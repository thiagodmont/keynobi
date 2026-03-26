import { type Extension, StateEffect, StateField, Prec, RangeSet } from "@codemirror/state";
import { EditorView, hoverTooltip, type Tooltip, Decoration, type DecorationSet, showTooltip, keymap } from "@codemirror/view";
import { linter, type Diagnostic as CmDiagnostic } from "@codemirror/lint";
import {
  autocompletion,
  type CompletionContext,
  type CompletionResult,
} from "@codemirror/autocomplete";
import { invoke } from "@tauri-apps/api/core";
import { updateDiagnostics, type Diagnostic } from "@/stores/lsp.store";
import { uriToPath, type LspHighlight } from "@/lib/tauri-api";

// ── Helpers ───────────────────────────────────────────────────────────────────

function lspLanguageId(language: string): string {
  switch (language) {
    case "kotlin":
    case "gradle":
      return "kotlin";
    default:
      return language;
  }
}

/** Convert a LSP 0-based line/character position to a CodeMirror doc offset. */
function lspPosToOffset(
  doc: EditorView["state"]["doc"],
  line: number,
  character: number
): number {
  const l = doc.line(Math.min(line + 1, doc.lines));
  return l.from + Math.min(character, l.length);
}

/** Navigate to an LSP definition/implementation result from the response. */
async function navigateToLspLocations(locations: unknown): Promise<void> {
  const { openFileAtLocation } = await import("@/services/project.service");
  const list = Array.isArray(locations)
    ? locations
    : locations
    ? [locations]
    : [];

  if (list.length === 0) return;

  const first = list[0] as any;
  const uri = first.uri ?? first.targetUri;
  const range = first.range ?? first.targetSelectionRange ?? first.targetRange;
  if (!uri || !range) return;

  const targetPath = uriToPath(uri);
  const targetLine = (range.start?.line ?? 0) + 1;
  const targetCol = range.start?.character ?? 0;
  await openFileAtLocation(targetPath, targetLine, targetCol);
}

// ── Definition link (Cmd+hover underline) ────────────────────────────────────

const setDefinitionLink = StateEffect.define<{ from: number; to: number } | null>();

const definitionLinkField = StateField.define<DecorationSet>({
  create: () => Decoration.none,
  update(deco, tr) {
    deco = deco.map(tr.changes);
    for (const e of tr.effects) {
      if (e.is(setDefinitionLink)) {
        if (e.value) {
          const mark = Decoration.mark({ class: "cm-definition-link" });
          return RangeSet.of([mark.range(e.value.from, e.value.to)]);
        }
        return Decoration.none;
      }
    }
    return deco;
  },
  provide: (f) => EditorView.decorations.from(f),
});

function lspDefinitionLinkExtension(path: string): Extension[] {
  let lastHoverPos: number | null = null;

  function clearLink(view: EditorView) {
    if (lastHoverPos !== null) {
      lastHoverPos = null;
      view.dispatch({ effects: setDefinitionLink.of(null) });
    }
  }

  return [
    definitionLinkField,
    EditorView.domEventHandlers({
      mousemove(event: MouseEvent, view: EditorView) {
        const isMac = navigator.platform.toLowerCase().includes("mac");
        const modKey = isMac ? event.metaKey : event.ctrlKey;
        if (!modKey) {
          clearLink(view);
          return false;
        }
        const pos = view.posAtCoords({ x: event.clientX, y: event.clientY });
        if (pos === null) {
          clearLink(view);
          return false;
        }
        if (pos === lastHoverPos) return false;
        lastHoverPos = pos;

        const word = view.state.wordAt(pos);
        if (word) {
          view.dispatch({ effects: setDefinitionLink.of({ from: word.from, to: word.to }) });
        } else {
          clearLink(view);
        }
        return false;
      },
      mouseleave(_event: MouseEvent, view: EditorView) {
        clearLink(view);
        return false;
      },
      keyup(event: KeyboardEvent, view: EditorView) {
        if (event.key === "Meta" || event.key === "Control") {
          clearLink(view);
        }
        return false;
      },
      click(event: MouseEvent, view: EditorView) {
        const isMac = navigator.platform.toLowerCase().includes("mac");
        const modKey = isMac ? event.metaKey : event.ctrlKey;
        if (!modKey) return false;

        const pos = view.posAtCoords({ x: event.clientX, y: event.clientY });
        if (pos === null) return false;

        const line = view.state.doc.lineAt(pos);
        const lspLine = line.number - 1;
        const lspCol = pos - line.from;

        event.preventDefault();
        event.stopPropagation();

        invoke("lsp_definition", { path, line: lspLine, col: lspCol })
          .then((result) => navigateToLspLocations(result))
          .catch(() => {});

        clearLink(view);
        return true;
      },
    }),
  ];
}

// ── Document highlight (mark occurrences) ────────────────────────────────────

const setDocumentHighlights = StateEffect.define<LspHighlight[]>();

const documentHighlightField = StateField.define<DecorationSet>({
  create: () => Decoration.none,
  update(deco, tr) {
    deco = deco.map(tr.changes);
    for (const e of tr.effects) {
      if (e.is(setDocumentHighlights)) {
        if (e.value.length === 0) return Decoration.none;
        const marks = e.value
          .map((h) => {
            const cls = h.kind === 3 ? "cm-highlight-write" : h.kind === 2 ? "cm-highlight-read" : "cm-highlight-text";
            const from = lspPosToOffset(tr.state.doc, h.range.start.line, h.range.start.character);
            const to = lspPosToOffset(tr.state.doc, h.range.end.line, h.range.end.character);
            if (from >= to) return null;
            return Decoration.mark({ class: cls }).range(from, to);
          })
          .filter((m): m is ReturnType<typeof Decoration.mark> & { from: number; to: number } => m !== null)
          .sort((a, b) => a.from - b.from);
        return marks.length > 0 ? RangeSet.of(marks) : Decoration.none;
      }
    }
    return deco;
  },
  provide: (f) => EditorView.decorations.from(f),
});

function lspDocumentHighlightExtension(path: string): Extension[] {
  let highlightTimer: ReturnType<typeof setTimeout> | undefined;

  return [
    documentHighlightField,
    EditorView.updateListener.of((update) => {
      if (!update.selectionSet) return;
      if (highlightTimer) clearTimeout(highlightTimer);
      highlightTimer = setTimeout(async () => {
        const pos = update.state.selection.main.head;
        const line = update.state.doc.lineAt(pos);
        const lspLine = line.number - 1;
        const lspCol = pos - line.from;

        // Only highlight if cursor is on a word character
        const charAt = update.state.sliceDoc(pos, pos + 1);
        if (!/[\w$]/.test(charAt)) {
          update.view.dispatch({ effects: setDocumentHighlights.of([]) });
          return;
        }

        try {
          const result = await invoke<LspHighlight[] | null>("lsp_document_highlight", { path, line: lspLine, col: lspCol });
          update.view.dispatch({ effects: setDocumentHighlights.of(result ?? []) });
        } catch {
          // LSP not ready
        }
      }, 250);
    }),
  ];
}

// ── Signature help ────────────────────────────────────────────────────────────

const setSigHelp = StateEffect.define<Tooltip | null>();

const sigHelpField = StateField.define<readonly Tooltip[]>({
  create: () => [],
  update(tooltips, tr) {
    for (const e of tr.effects) {
      if (e.is(setSigHelp)) {
        return e.value ? [e.value] : [];
      }
    }
    return tooltips;
  },
  provide: (f) => showTooltip.computeN([f], (state) => state.field(f)),
});

function lspSignatureHelpExtension(path: string): Extension[] {
  return [
    sigHelpField,
    EditorView.updateListener.of((update) => {
      if (!update.docChanged) return;

      // Check if the last inserted character is ( or ,
      let triggerChar = "";
      update.changes.iterChanges((_fromA, _toA, _fromB, toB, inserted) => {
        const text = inserted.toString();
        const last = text[text.length - 1];
        if (last === "(" || last === ",") triggerChar = last;
        if (last === ")") triggerChar = ")";
      });

      if (triggerChar === ")") {
        update.view.dispatch({ effects: setSigHelp.of(null) });
        return;
      }
      if (!triggerChar) return;

      const pos = update.state.selection.main.head;
      const line = update.state.doc.lineAt(pos);
      const lspLine = line.number - 1;
      const lspCol = pos - line.from;

      invoke<any>("lsp_signature_help", { path, line: lspLine, col: lspCol })
        .then((result) => {
          if (!result || !result.signatures?.length) {
            update.view.dispatch({ effects: setSigHelp.of(null) });
            return;
          }
          const activeIdx = result.activeSignature ?? 0;
          const sig = result.signatures[activeIdx];
          const activeParam = result.activeParameter ?? sig.activeParameter ?? 0;
          const params: string[] = (sig.parameters ?? []).map((p: any) =>
            typeof p.label === "string" ? p.label : sig.label.slice(p.label[0], p.label[1])
          );

          const dom = document.createElement("div");
          dom.className = "cm-sig-help";
          dom.style.cssText =
            "padding:4px 8px;font-size:12px;font-family:var(--font-mono);max-width:600px;white-space:pre-wrap;line-height:1.5;";

          if (params.length > 0) {
            params.forEach((param, i) => {
              if (i > 0) dom.appendChild(document.createTextNode(", "));
              if (i === activeParam) {
                const strong = document.createElement("strong");
                strong.style.color = "var(--accent)";
                strong.textContent = param;
                dom.appendChild(strong);
              } else {
                dom.appendChild(document.createTextNode(param));
              }
            });
          } else {
            dom.textContent = sig.label;
          }

          const tooltip: Tooltip = {
            pos: update.state.selection.main.head,
            above: true,
            strictSide: true,
            arrow: false,
            create: () => ({ dom }),
          };
          update.view.dispatch({ effects: setSigHelp.of(tooltip) });
        })
        .catch(() => {});
    }),
  ];
}

// ── Navigation keymaps (F12, Shift+F12, Cmd+F12, Cmd+.) ──────────────────────

function lspNavigationKeymaps(path: string): Extension {
  return Prec.highest(
    keymap.of([
      {
        key: "F12",
        run(view) {
          const pos = view.state.selection.main.head;
          const line = view.state.doc.lineAt(pos);
          invoke("lsp_definition", { path, line: line.number - 1, col: pos - line.from })
            .then((result) => navigateToLspLocations(result))
            .catch(() => {});
          return true;
        },
      },
      {
        key: "Shift-F12",
        async run(view) {
          const pos = view.state.selection.main.head;
          const line = view.state.doc.lineAt(pos);
          try {
            const result = await invoke<any[]>("lsp_references", {
              path,
              line: line.number - 1,
              col: pos - line.from,
            });
            if (!result?.length) return true;
            const { showReferences } = await import("@/stores/references.store");
            const word = view.state.wordAt(pos);
            const query = word ? view.state.sliceDoc(word.from, word.to) : "references";
            showReferences(query, result);
          } catch {
            // LSP not ready
          }
          return true;
        },
      },
      {
        key: "Mod-F12",
        run(view) {
          const pos = view.state.selection.main.head;
          const line = view.state.doc.lineAt(pos);
          invoke("lsp_implementation", { path, line: line.number - 1, col: pos - line.from })
            .then((result) => navigateToLspLocations(result))
            .catch(() => {});
          return true;
        },
      },
      {
        key: "Mod-.",
        async run(view) {
          const pos = view.state.selection.main.head;
          const line = view.state.doc.lineAt(pos);
          const selFrom = view.state.selection.main.from;
          const selTo = view.state.selection.main.to;
          const selLine = view.state.doc.lineAt(selFrom);
          const selEndLine = view.state.doc.lineAt(selTo);
          try {
            const actions = await invoke<any[]>("lsp_code_action", {
              path,
              startLine: selLine.number - 1,
              startCol: selFrom - selLine.from,
              endLine: selEndLine.number - 1,
              endCol: selTo - selEndLine.from,
            });
            if (!actions?.length) return true;
            const { showCodeActions } = await import("@/stores/references.store");
            showCodeActions(actions, pos);
          } catch {
            // LSP not ready
          }
          return true;
        },
      },
    ])
  );
}

// ── Main export ───────────────────────────────────────────────────────────────

export function createLspExtension(path: string, language: string): Extension[] {
  const langId = lspLanguageId(language);
  const isLspLanguage = langId === "kotlin";

  if (!isLspLanguage) return [];

  return [
    lspDiagnosticsSource(path),
    lspCompletionSource(path),
    lspHoverExtension(path),
    lspDidChangeListener(path),
    ...lspDefinitionLinkExtension(path),
    ...lspDocumentHighlightExtension(path),
    ...lspSignatureHelpExtension(path),
    lspNavigationKeymaps(path),
  ];
}

// ── Existing extensions ───────────────────────────────────────────────────────

function lspDiagnosticsSource(path: string): Extension {
  return linter(
    async (view) => {
      try {
        const result = await invoke<any>("lsp_pull_diagnostics", { path });
        const items = result?.items ?? result?.diagnostics ?? [];
        const diagnostics: CmDiagnostic[] = [];
        const storeDiags: Diagnostic[] = [];

        for (const item of items) {
          const range = item.range;
          if (!range) continue;

          const fromLine = view.state.doc.line(
            Math.min(range.start.line + 1, view.state.doc.lines)
          );
          const toLine = view.state.doc.line(
            Math.min(range.end.line + 1, view.state.doc.lines)
          );
          const from = fromLine.from + Math.min(range.start.character, fromLine.length);
          const to = toLine.from + Math.min(range.end.character, toLine.length);

          const severity = mapSeverity(item.severity);

          diagnostics.push({
            from: Math.max(0, from),
            to: Math.max(from, to),
            severity,
            message: item.message ?? "Unknown error",
            source: item.source ?? "kotlin",
          });

          storeDiags.push({
            path,
            range: {
              startLine: range.start.line,
              startCol: range.start.character,
              endLine: range.end.line,
              endCol: range.end.character,
            },
            severity: severity as Diagnostic["severity"],
            message: item.message ?? "Unknown error",
            source: item.source ?? null,
            code: item.code?.toString() ?? null,
          });
        }

        updateDiagnostics(path, storeDiags);
        return diagnostics;
      } catch {
        return [];
      }
    },
    { delay: 1000 }
  );
}

function lspCompletionSource(path: string): Extension {
  return autocompletion({
    override: [
      async (context: CompletionContext): Promise<CompletionResult | null> => {
        const explicit = context.explicit;
        const before = context.matchBefore(/[\w.]/);
        if (!explicit && !before) return null;

        const pos = context.pos;
        const line = context.state.doc.lineAt(pos);
        const lineNum = line.number - 1;
        const col = pos - line.from;

        try {
          const result = await invoke<any>("lsp_complete", {
            path,
            line: lineNum,
            col,
          });

          const items = result?.items ?? result ?? [];
          if (!Array.isArray(items) || items.length === 0) return null;

          const from = before ? before.from : pos;

          return {
            from,
            options: items.map((item: any) => ({
              label: item.label ?? "",
              detail: item.detail ?? undefined,
              type: mapCompletionKind(item.kind),
              apply: item.insertText ?? item.textEdit?.newText ?? item.label,
              boost: item.sortText
                ? -item.sortText.charCodeAt(0)
                : undefined,
            })),
          };
        } catch {
          return null;
        }
      },
    ],
  });
}

function lspHoverExtension(path: string): Extension {
  return hoverTooltip(
    async (view, pos): Promise<Tooltip | null> => {
      const line = view.state.doc.lineAt(pos);
      const lineNum = line.number - 1;
      const col = pos - line.from;

      try {
        const result = await invoke<any>("lsp_hover", {
          path,
          line: lineNum,
          col,
        });

        if (!result) return null;

        const contents =
          typeof result.contents === "string"
            ? result.contents
            : result.contents?.value ?? JSON.stringify(result.contents);

        if (!contents || contents === "null") return null;

        return {
          pos,
          above: true,
          create: () => {
            const dom = document.createElement("div");
            dom.style.cssText =
              "max-width:500px;max-height:300px;overflow:auto;padding:4px 8px;font-size:12px;font-family:var(--font-mono);white-space:pre-wrap;line-height:1.4;";
            dom.textContent = contents.replace(/```\w*\n?/g, "").replace(/```/g, "");
            return { dom };
          },
        };
      } catch {
        return null;
      }
    },
    { hoverTime: 500 }
  );
}

function lspDidChangeListener(path: string): Extension {
  let timer: ReturnType<typeof setTimeout> | undefined;
  let version = 0;

  return EditorView.updateListener.of((update) => {
    if (!update.docChanged) return;
    version++;

    if (timer) clearTimeout(timer);
    timer = setTimeout(async () => {
      const content = update.state.doc.toString();
      try {
        await invoke("lsp_did_change", {
          path,
          content,
          version,
        });
      } catch {
        // LSP not running — ignore
      }
    }, 300);
  });
}

function mapSeverity(lspSeverity: number | undefined): "error" | "warning" | "info" | "hint" {
  switch (lspSeverity) {
    case 1:
      return "error";
    case 2:
      return "warning";
    case 3:
      return "info";
    case 4:
      return "hint";
    default:
      return "info";
  }
}

function mapCompletionKind(kind: number | undefined): string | undefined {
  const kindMap: Record<number, string> = {
    1: "text",
    2: "method",
    3: "function",
    4: "constructor",
    5: "field",
    6: "variable",
    7: "class",
    8: "interface",
    9: "namespace",
    10: "property",
    11: "type",
    12: "keyword",
    13: "constant",
    14: "enum",
    15: "enum",
  };
  return kind ? kindMap[kind] : undefined;
}
