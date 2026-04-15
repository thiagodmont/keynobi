/** Toolbar labels for Build / LogViewer: filtered subset vs full buffer in the panel. */
export function formatBuildLogToolbarCount(params: {
  filterActive: boolean;
  visible: number;
  total: number;
}): { text: string; title: string } {
  const { filterActive, visible, total } = params;
  const v = visible.toLocaleString();
  const t = total.toLocaleString();

  if (!filterActive) {
    return {
      text: `${t} lines`,
      title: "Total build output lines kept in this panel.",
    };
  }

  if (visible !== total) {
    return {
      text: `${v} / ${t}`,
      title:
        "First: lines matching level, source, and search. Second: all lines in the build log buffer.",
    };
  }

  return {
    text: `${t} lines`,
    title: "Every line in the buffer matches the current toolbar filters.",
  };
}
