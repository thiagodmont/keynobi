import { EditorView } from "@codemirror/view";
import { HighlightStyle, syntaxHighlighting } from "@codemirror/language";
import { tags as t } from "@lezer/highlight";
import type { Extension } from "@codemirror/state";

const editorTheme = EditorView.theme(
  {
    "&": {
      color: "#cccccc",
      backgroundColor: "#1e1e1e",
      height: "100%",
      fontSize: "var(--font-size-editor)",
      fontFamily: "var(--font-mono)",
    },
    ".cm-content": {
      caretColor: "#aeafad",
      padding: "0",
    },
    "&.cm-focused .cm-cursor": {
      borderLeftColor: "#aeafad",
    },
    "&.cm-focused .cm-selectionBackground, .cm-selectionBackground": {
      backgroundColor: "#264f78",
    },
    ".cm-gutters": {
      backgroundColor: "#1e1e1e",
      color: "#858585",
      border: "none",
      borderRight: "1px solid #2d2d30",
    },
    ".cm-gutter.cm-lineNumbers .cm-gutterElement": {
      paddingRight: "16px",
      minWidth: "40px",
    },
    ".cm-activeLineGutter": {
      backgroundColor: "#282828",
    },
    ".cm-activeLine": {
      backgroundColor: "rgba(255,255,255,0.04)",
    },
    ".cm-matchingBracket": {
      backgroundColor: "rgba(255,255,255,0.12)",
      outline: "none",
    },
    ".cm-nonmatchingBracket": {
      color: "#f14c4c",
    },
    ".cm-tooltip": {
      border: "1px solid #454545",
      backgroundColor: "#252526",
      color: "#cccccc",
    },
    ".cm-tooltip-autocomplete": {
      "& > ul > li": {
        padding: "2px 8px",
      },
      "& > ul > li[aria-selected]": {
        backgroundColor: "#094771",
        color: "#cccccc",
      },
    },
    ".cm-panels": {
      backgroundColor: "#2d2d30",
      color: "#cccccc",
    },
    ".cm-panels.cm-panels-top": {
      borderBottom: "2px solid var(--border)",
    },
    ".cm-searchMatch": {
      backgroundColor: "#9e6a03",
      outline: "1px solid #f1a10a",
    },
    ".cm-searchMatch.cm-searchMatch-selected": {
      backgroundColor: "#b17a21",
    },
    ".cm-selectionMatch": {
      backgroundColor: "#add6ff26",
    },
    ".cm-foldPlaceholder": {
      backgroundColor: "transparent",
      border: "none",
      color: "#858585",
    },
    ".cm-scroller": {
      fontFamily: "var(--font-mono)",
    },
    // Fold gutter
    ".cm-foldGutter span": {
      color: "#858585",
    },
  },
  { dark: true }
);

const highlightStyle = HighlightStyle.define([
  { tag: t.keyword, color: "#569cd6" },
  { tag: [t.name, t.deleted, t.character, t.macroName], color: "#cccccc" },
  { tag: [t.propertyName], color: "#9cdcfe" },
  { tag: [t.function(t.variableName), t.labelName], color: "#dcdcaa" },
  { tag: [t.color, t.constant(t.name), t.standard(t.name)], color: "#569cd6" },
  { tag: [t.definition(t.name), t.separator], color: "#cccccc" },
  { tag: [t.typeName, t.className, t.number, t.changed, t.annotation, t.modifier, t.self, t.namespace], color: "#4ec9b0" },
  { tag: [t.operator, t.operatorKeyword, t.url, t.escape, t.regexp, t.link, t.special(t.string)], color: "#d4d4d4" },
  { tag: [t.meta, t.comment], color: "#6a9955", fontStyle: "italic" },
  { tag: t.strong, fontWeight: "bold" },
  { tag: t.emphasis, fontStyle: "italic" },
  { tag: t.strikethrough, textDecoration: "line-through" },
  { tag: t.link, color: "#569cd6", textDecoration: "underline" },
  { tag: t.heading, fontWeight: "bold", color: "#569cd6" },
  { tag: [t.atom, t.bool, t.special(t.variableName)], color: "#569cd6" },
  { tag: [t.processingInstruction, t.string, t.inserted], color: "#ce9178" },
  { tag: t.invalid, color: "#f44747" },
  { tag: t.number, color: "#b5cea8" },
]);

export const editorThemeExtension: Extension = [
  editorTheme,
  syntaxHighlighting(highlightStyle),
];
