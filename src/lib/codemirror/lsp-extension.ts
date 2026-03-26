import { type Extension, StateEffect, StateField, Prec, RangeSet } from "@codemirror/state";
import { EditorView, hoverTooltip, type Tooltip, Decoration, type DecorationSet, showTooltip, keymap } from "@codemirror/view";
import { linter, type Diagnostic as CmDiagnostic } from "@codemirror/lint";
import {
  autocompletion,
  type CompletionContext,
  type CompletionResult,
} from "@codemirror/autocomplete";
import { invoke } from "@tauri-apps/api/core";
import { updateDiagnostics, type Diagnostic, hasCapability } from "@/stores/lsp.store";
import { type LspHighlight } from "@/lib/tauri-api";
import { navLog } from "@/lib/navigation-logger";

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
async function navigateToLspLocations(
  locations: unknown,
  context: string,
): Promise<void> {
  const { openFileAtLocation, openVirtualFile } = await import("@/services/project.service");
  const { parseLspUri } = await import("@/lib/tauri-api");

  // Log the raw response at trace level — invaluable when the server returns
  // null or an unexpected shape without any error.
  navLog(
    "debug",
    `${context} raw response: ${JSON.stringify(locations) ?? "null"}`
  );

  const list = Array.isArray(locations)
    ? locations
    : locations
    ? [locations]
    : [];

  if (list.length === 0) {
    const { lspState } = await import("@/stores/lsp.store");
    const { showToast } = await import("@/components/common/Toast");
    const lspStatus = lspState.status.state;
    if (lspStatus === "indexing") {
      showToast("LSP is still loading the project — try again in a moment", "info");
    } else {
      showToast("No definition found at this position", "info");
    }
    navLog("warn", `${context} → no location returned by server (LSP state: ${lspStatus})`);
    return;
  }

  const first = list[0] as any;
  const uri = first.uri ?? first.targetUri;
  const range = first.range ?? first.targetSelectionRange ?? first.targetRange;
  if (!uri || !range) {
    navLog("warn", `${context} → server response missing uri/range`);
    return;
  }

  const targetLine = (range.start?.line ?? 0) + 1;
  const targetCol = range.start?.character ?? 0;

  const parsed = parseLspUri(uri);
  navLog("info", `${context} → ${uri} kind=${parsed.kind} line=${targetLine} col=${targetCol}`);

  if (parsed.kind === "file") {
    await openFileAtLocation(parsed.path, targetLine, targetCol);
  } else if (parsed.kind === "jar" || parsed.kind === "jrt") {
    await openVirtualFile(uri, targetLine, targetCol);
  } else {
    // Unknown URI — attempt as a plain path (best-effort fallback).
    navLog("warn", `${context} → unknown URI scheme, trying as plain path`);
    await openFileAtLocation(uri, targetLine, targetCol);
  }

  navLog("debug", `${context} → navigation complete`);
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

        navLog("debug", `Cmd+click → requesting definition ${path}:${lspLine}:${lspCol}`);

        invoke("lsp_definition", { path, line: lspLine, col: lspCol })
          .then((result) =>
            navigateToLspLocations(result, `definition ${path}:${lspLine}:${lspCol}`)
          )
          .catch((err) => {
            navLog("warn", `Cmd+click definition failed: ${err}`);
          });

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
          .flatMap((h) => {
            const cls = h.kind === 3 ? "cm-highlight-write" : h.kind === 2 ? "cm-highlight-read" : "cm-highlight-text";
            const from = lspPosToOffset(tr.state.doc, h.range.start.line, h.range.start.character);
            const to = lspPosToOffset(tr.state.doc, h.range.end.line, h.range.end.character);
            if (from >= to) return [];
            return [Decoration.mark({ class: cls }).range(from, to)];
          })
          .sort((a, b) => a.from - b.from);
        return marks.length > 0 ? Decoration.set(marks) : Decoration.none;
      }
    }
    return deco;
  },
  provide: (f) => EditorView.decorations.from(f),
});

function lspDocumentHighlightExtension(path: string): Extension[] {
  let highlightTimer: ReturnType<typeof setTimeout> | undefined;
  // Set to true once we know the server doesn't support this method.
  // Prevents spamming the server and stops ERROR noise in the Output panel.
  let unsupported = false;

  return [
    documentHighlightField,
    EditorView.updateListener.of((update) => {
      if (!update.selectionSet || unsupported) return;
      if (highlightTimer) clearTimeout(highlightTimer);
      highlightTimer = setTimeout(async () => {
        // Check the capability gate populated from the initialize response.
        // If the server omitted documentHighlightProvider, stop immediately.
        if (!hasCapability("documentHighlightProvider")) {
          unsupported = true;
          update.view.dispatch({ effects: setDocumentHighlights.of([]) });
          return;
        }

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
        } catch (err) {
          // If the server returns "no handler" or any method-not-found error,
          // permanently disable highlighting for this session so it stops
          // generating ERROR noise in the Output panel.
          const msg = String(err).toLowerCase();
          if (
            msg.includes("no handler") ||
            msg.includes("not supported") ||
            msg.includes("method not found") ||
            msg.includes("-32601") // JSON-RPC MethodNotFound code
          ) {
            unsupported = true;
          }
          update.view.dispatch({ effects: setDocumentHighlights.of([]) });
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
      update.changes.iterChanges((_fromA, _toA, _fromB, _toB, inserted) => {
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
          const lspLine = line.number - 1;
          const lspCol = pos - line.from;
          navLog("debug", `F12 → requesting definition ${path}:${lspLine}:${lspCol}`);
          invoke("lsp_definition", { path, line: lspLine, col: lspCol })
            .then((result) =>
              navigateToLspLocations(result, `definition ${path}:${lspLine}:${lspCol}`)
            )
            .catch((err) => navLog("warn", `F12 definition failed: ${err}`));
          return true;
        },
      },
      {
        key: "Shift-F12",
        run(view) {
          const pos = view.state.selection.main.head;
          const line = view.state.doc.lineAt(pos);
          const lspLine = line.number - 1;
          const lspCol = pos - line.from;
          navLog("debug", `Shift+F12 → requesting references ${path}:${lspLine}:${lspCol}`);
          invoke<any[]>("lsp_references", { path, line: lspLine, col: lspCol })
            .then(async (result) => {
              if (!result?.length) {
                navLog("warn", `Shift+F12 → no references found`);
                return;
              }
              navLog("info", `Shift+F12 → ${result.length} reference(s) found`);
              const { showReferences } = await import("@/stores/references.store");
              const word = view.state.wordAt(pos);
              const query = word ? view.state.sliceDoc(word.from, word.to) : "references";
              showReferences(query, result);
            })
            .catch((err) => navLog("warn", `Shift+F12 references failed: ${err}`));
          return true;
        },
      },
      {
        key: "Mod-F12",
        run(view) {
          const pos = view.state.selection.main.head;
          const line = view.state.doc.lineAt(pos);
          const lspLine = line.number - 1;
          const lspCol = pos - line.from;
          navLog("debug", `Cmd+F12 → requesting implementation ${path}:${lspLine}:${lspCol}`);
          invoke("lsp_implementation", { path, line: lspLine, col: lspCol })
            .then((result) =>
              navigateToLspLocations(result, `implementation ${path}:${lspLine}:${lspCol}`)
            )
            .catch((err) => navLog("warn", `Cmd+F12 implementation failed: ${err}`));
          return true;
        },
      },
      {
        key: "Mod-.",
        run(view) {
          const pos = view.state.selection.main.head;
          const selFrom = view.state.selection.main.from;
          const selTo = view.state.selection.main.to;
          const selLine = view.state.doc.lineAt(selFrom);
          const selEndLine = view.state.doc.lineAt(selTo);
          invoke<any[]>("lsp_code_action", {
            path,
            startLine: selLine.number - 1,
            startCol: selFrom - selLine.from,
            endLine: selEndLine.number - 1,
            endCol: selTo - selEndLine.from,
          })
            .then(async (actions) => {
              if (!actions?.length) return;
              const { showCodeActions } = await import("@/stores/references.store");
              showCodeActions(actions, pos);
            })
            .catch(() => { /* LSP not ready */ });
          return true;
        },
      },
    ])
  );
}

// ── Semantic Highlighting ─────────────────────────────────────────────────────

/**
 * LSP semantic token types (index matches the server's legend order).
 * We map each index to a CSS class applied as a CodeMirror decoration.
 *
 * The mapping follows the standard LSP token type names we declared in the
 * `initialize` capabilities, so the server's legend always matches this list.
 */
const SEMANTIC_TOKEN_TYPES = [
  "namespace",    // 0
  "type",         // 1
  "class",        // 2
  "enum",         // 3
  "interface",    // 4
  "struct",       // 5
  "typeParameter",// 6
  "parameter",    // 7
  "variable",     // 8
  "property",     // 9
  "enumMember",   // 10
  "event",        // 11
  "function",     // 12
  "method",       // 13
  "macro",        // 14
  "keyword",      // 15
  "modifier",     // 16
  "comment",      // 17
  "string",       // 18
  "number",       // 19
  "regexp",       // 20
  "operator",     // 21
  "decorator",    // 22
];

const SEMANTIC_TOKEN_MODIFIERS = [
  "declaration",   // bit 0
  "definition",    // bit 1
  "readonly",      // bit 2
  "static",        // bit 3
  "deprecated",    // bit 4
  "abstract",      // bit 5
  "async",         // bit 6
  "modification",  // bit 7
  "documentation", // bit 8
  "defaultLibrary",// bit 9
];

/**
 * Build a list of CSS class names for a single semantic token.
 * The base class is `cm-semantic-<tokenType>` (e.g. `cm-semantic-class`).
 * Active modifiers add extra classes (e.g. `cm-semantic-deprecated`).
 */
function semanticTokenClasses(
  tokenType: number,
  modifiers: number,
  serverLegendTypes?: string[],
  serverLegendModifiers?: string[]
): string[] {
  const types = serverLegendTypes ?? SEMANTIC_TOKEN_TYPES;
  const mods  = serverLegendModifiers ?? SEMANTIC_TOKEN_MODIFIERS;
  const classes: string[] = [];

  const typeName = types[tokenType];
  if (typeName) classes.push(`cm-semantic-${typeName}`);

  for (let i = 0; i < mods.length; i++) {
    if (modifiers & (1 << i)) {
      classes.push(`cm-semantic-${mods[i]}`);
    }
  }

  return classes;
}

const setSemanticDecorations = StateEffect.define<DecorationSet>();

const semanticDecorationField = StateField.define<DecorationSet>({
  create: () => Decoration.none,
  update(deco, tr) {
    deco = deco.map(tr.changes);
    for (const e of tr.effects) {
      if (e.is(setSemanticDecorations)) return e.value;
    }
    return deco;
  },
  provide: (f) => EditorView.decorations.from(f),
});

/**
 * Decode the LSP relative-encoded semantic token data into decoration ranges.
 *
 * LSP encodes tokens as a flat array of 5-tuples:
 *   [deltaLine, deltaStartChar, length, tokenType, tokenModifiers]
 * Each position is relative to the previous token (hence "relative").
 */
function decodeSemanticTokens(
  data: number[],
  doc: EditorView["state"]["doc"],
  serverLegendTypes?: string[],
  serverLegendModifiers?: string[]
): import("@codemirror/state").Range<Decoration>[] {
  const marks: import("@codemirror/state").Range<Decoration>[] = [];

  let line = 0;
  let startChar = 0;

  for (let i = 0; i + 4 < data.length; i += 5) {
    const deltaLine      = data[i];
    const deltaStartChar = data[i + 1];
    const length         = data[i + 2];
    const tokenType      = data[i + 3];
    const tokenMods      = data[i + 4];

    if (deltaLine > 0) {
      line += deltaLine;
      startChar = deltaStartChar;
    } else {
      startChar += deltaStartChar;
    }

    const classes = semanticTokenClasses(tokenType, tokenMods, serverLegendTypes, serverLegendModifiers);
    if (classes.length === 0) continue;

    // Map LSP 0-based line/col to CodeMirror offset.
    const cmLine = Math.min(line + 1, doc.lines);
    const docLine = doc.line(cmLine);
    const from = docLine.from + Math.min(startChar, docLine.length);
    const to   = Math.min(from + length, docLine.to);
    if (from >= to) continue;

    marks.push(Decoration.mark({ class: classes.join(" ") }).range(from, to));
  }

  return marks.sort((a, b) => a.from - b.from);
}

function lspSemanticHighlightExtension(path: string): Extension[] {
  let timer: ReturnType<typeof setTimeout> | undefined;
  // Set true when the server reports it doesn't support semantic tokens so we
  // stop making requests for the lifetime of the editor session.
  let unsupported = false;

  return [
    semanticDecorationField,
    EditorView.updateListener.of((update) => {
      if (!update.docChanged && !update.transactions.some((t) => t.isUserEvent("input"))) {
        // Only refresh after document changes.
        return;
      }
      if (unsupported) return;
      if (timer) clearTimeout(timer);
      timer = setTimeout(async () => {
        if (unsupported) return;
        try {
          const result = await invoke<any>("lsp_semantic_tokens", { path });
          if (!result) return;

          const data: number[] = result.data ?? [];
          if (!Array.isArray(data) || data.length === 0) return;

          // Use server legend if available; fall back to our declared order.
          const { lspState } = await import("@/stores/lsp.store");
          const caps = lspState.serverCapabilities as any;
          const legend = caps?.semanticTokensProvider?.legend;
          const types = legend?.tokenTypes as string[] | undefined;
          const mods  = legend?.tokenModifiers as string[] | undefined;

          const doc = update.view.state.doc;
          const marks = decodeSemanticTokens(data, doc, types, mods);
          const decoSet: DecorationSet = marks.length > 0
            ? Decoration.set(marks)
            : Decoration.none;

          update.view.dispatch({
            effects: setSemanticDecorations.of(decoSet),
          });
        } catch (err) {
          const msg = String(err).toLowerCase();
          if (
            msg.includes("method not found") ||
            msg.includes("-32601") ||
            msg.includes("not supported") ||
            msg.includes("not running")
          ) {
            unsupported = true;
          }
        }
      }, 500);
    }),
  ];
}

// ── Main export ───────────────────────────────────────────────────────────────

/**
 * Apply an LSP `WorkspaceEdit` to the open editor files.
 *
 * A `WorkspaceEdit` maps file URIs to arrays of `TextEdit` objects.
 * This function applies each edit to the corresponding file.  If the file
 * is currently open in the editor, the edit is applied via a CodeMirror
 * transaction so the user can undo it.  For background files, the content
 * is written to disk directly.
 */
export async function applyWorkspaceEdit(workspaceEdit: any): Promise<void> {
  const changes: Record<string, any[]> = workspaceEdit.changes ?? {};
  const documentChanges: any[] = workspaceEdit.documentChanges ?? [];

  // Normalize to a map of uri → TextEdit[].
  const edits = new Map<string, any[]>();

  for (const [uri, textEdits] of Object.entries(changes)) {
    edits.set(uri, textEdits as any[]);
  }
  for (const dc of documentChanges) {
    if (dc.textDocument?.uri && dc.edits) {
      edits.set(dc.textDocument.uri, dc.edits);
    }
  }

  const { parseLspUri } = await import("@/lib/tauri-api");
  const { writeFile } = await import("@/lib/tauri-api");
  const { editorState, saveEditorState } = await import("@/stores/editor.store");
  const { getEditorView } = await import("@/components/editor/CodeEditor");

  for (const [uri, textEdits] of edits) {
    const parsed = parseLspUri(uri);
    if (parsed.kind !== "file") continue;
    const filePath = parsed.path;

    // Apply to the active editor view if this file is currently active.
    const activeView = getEditorView();
    if (editorState.activeFilePath === filePath && activeView) {
      const doc = activeView.state.doc;
      // Convert LSP TextEdits to CodeMirror changes.
      // Sort in reverse order so offsets don't shift during application.
      const sortedEdits = [...textEdits].sort((a: any, b: any) => {
        const aLine = a.range.start.line;
        const bLine = b.range.start.line;
        if (aLine !== bLine) return bLine - aLine;
        return b.range.start.character - a.range.start.character;
      });

      const changes = sortedEdits.map((te: any) => {
        const fromLine = doc.line(Math.min(te.range.start.line + 1, doc.lines));
        const toLine   = doc.line(Math.min(te.range.end.line + 1, doc.lines));
        const from = fromLine.from + Math.min(te.range.start.character, fromLine.length);
        const to   = toLine.from   + Math.min(te.range.end.character, toLine.length);
        return { from, to, insert: te.newText ?? "" };
      });

      activeView.dispatch({ changes });
      saveEditorState(filePath, activeView.state);
    } else {
      // Background file: read, apply edits, write back.
      try {
        const { readFile } = await import("@/lib/tauri-api");
        const content = await readFile(filePath);
        const lines = content.split("\n");
        const sortedEdits = [...textEdits].sort((a: any, b: any) => {
          if (a.range.start.line !== b.range.start.line) {
            return b.range.start.line - a.range.start.line;
          }
          return b.range.start.character - a.range.start.character;
        });
        // Simple line-based apply (good enough for organize imports).
        let result = content;
        for (const te of sortedEdits as any[]) {
          const startLine = te.range.start.line;
          const endLine   = te.range.end.line;
          const startChar = te.range.start.character;
          const endChar   = te.range.end.character;
          const before = lines.slice(0, startLine).join("\n") +
            (startLine > 0 ? "\n" : "") +
            lines[startLine].slice(0, startChar);
          const after  = lines[endLine].slice(endChar) +
            (endLine < lines.length - 1 ? "\n" : "") +
            lines.slice(endLine + 1).join("\n");
          result = before + (te.newText ?? "") + after;
        }
        await writeFile(filePath, result);
      } catch {
        // Best-effort; if we can't read/write, skip silently.
      }
    }
  }
}

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
    ...lspSemanticHighlightExtension(path),
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
