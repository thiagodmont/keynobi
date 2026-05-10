export interface LogcatPreset {
  name: string;
  query: string;
  builtin: true;
}

export const BUILTIN_LOGCAT_FILTER_PRESETS: readonly LogcatPreset[] = [
  { name: "My App", query: "package:mine", builtin: true },
  { name: "Crashes", query: "is:crash", builtin: true },
  { name: "Errors+", query: "level:error", builtin: true },
  { name: "Last 5 min", query: "age:5m", builtin: true },
  { name: "My App OR Crashes", query: "package:mine | is:crash", builtin: true },
];

export function commitSavedFilterQuery(query: string): string {
  return `${query.trimEnd()} `;
}
