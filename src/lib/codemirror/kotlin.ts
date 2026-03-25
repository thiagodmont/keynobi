import { StreamLanguage, LanguageSupport } from "@codemirror/language";
import { clike } from "@codemirror/legacy-modes/mode/clike";

const kotlinKeywords = [
  "fun", "val", "var", "class", "interface", "object", "when", "if", "else",
  "for", "while", "do", "return", "break", "continue", "is", "in", "as",
  "by", "constructor", "init", "companion", "data", "sealed", "enum",
  "abstract", "open", "override", "private", "protected", "internal",
  "public", "suspend", "inline", "crossinline", "noinline", "reified",
  "expect", "actual", "annotation", "typealias", "import", "package",
  "try", "catch", "finally", "throw", "null", "true", "false", "this",
  "super", "it", "typeof", "where", "dynamic", "external", "lateinit",
  "tailrec", "operator", "infix", "vararg", "const", "delegate",
];

const kotlinTypes = [
  "Int", "Long", "Short", "Byte", "Float", "Double", "Boolean", "Char",
  "String", "Unit", "Nothing", "Any", "Array", "List", "Map", "Set",
  "MutableList", "MutableMap", "MutableSet", "Pair", "Triple",
  "Collection", "Iterable", "Sequence", "Flow", "StateFlow", "SharedFlow",
];

const kotlinMode = clike({
  name: "kotlin",
  keywords: kotlinKeywords.reduce((acc: Record<string, string>, kw) => {
    acc[kw] = "keyword";
    return acc;
  }, {}),
  types: kotlinTypes.reduce((acc: Record<string, string>, t) => {
    acc[t] = "variable-3";
    return acc;
  }, {}),
  atoms: { true: "atom", false: "atom", null: "atom" },
  hooks: {
    "@": function () {
      return "meta";
    },
  },
  multiLineStrings: true,
});

export const kotlinLanguage = StreamLanguage.define(kotlinMode);

export function kotlin(): LanguageSupport {
  return new LanguageSupport(kotlinLanguage);
}
