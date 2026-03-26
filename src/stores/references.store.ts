import { createStore } from "solid-js/store";
import { setActiveBottomTab, setUIState } from "@/stores/ui.store";

export interface ReferenceItem {
  path: string;
  line: number;
  col: number;
  /** 1-based line number for display */
  displayLine: number;
  lineContent: string;
  uri: string;
}

export interface ReferenceGroup {
  path: string;
  relativePath: string;
  items: ReferenceItem[];
}

export interface CodeActionItem {
  title: string;
  kind?: string;
  edit?: any;
}

interface ReferencesState {
  query: string;
  groups: ReferenceGroup[];
  totalCount: number;
  visible: boolean;
  /** For code actions popup */
  codeActions: CodeActionItem[];
  codeActionsPos: number;
  codeActionsVisible: boolean;
}

const [referencesState, setReferencesState] = createStore<ReferencesState>({
  query: "",
  groups: [],
  totalCount: 0,
  visible: false,
  codeActions: [],
  codeActionsPos: 0,
  codeActionsVisible: false,
});

export { referencesState };

function uriToRelativePath(uri: string, projectRoot?: string): string {
  const path = uri.replace(/^file:\/\//, "");
  if (projectRoot && path.startsWith(projectRoot)) {
    return path.slice(projectRoot.length + 1);
  }
  const parts = path.split("/");
  return parts.slice(-3).join("/");
}

export function showReferences(query: string, lspLocations: any[]): void {
  // Group by file
  const byFile = new Map<string, ReferenceItem[]>();
  for (const loc of lspLocations) {
    const uri = loc.uri ?? "";
    const path = uri.replace(/^file:\/\//, "");
    const line = loc.range?.start?.line ?? 0;
    const col = loc.range?.start?.character ?? 0;
    if (!byFile.has(uri)) byFile.set(uri, []);
    byFile.get(uri)!.push({
      path,
      line: line,
      col,
      displayLine: line + 1,
      lineContent: "",
      uri,
    });
  }

  const groups: ReferenceGroup[] = [];
  byFile.forEach((items, uri) => {
    const path = uri.replace(/^file:\/\//, "");
    groups.push({
      path,
      relativePath: uriToRelativePath(uri),
      items,
    });
  });

  const totalCount = lspLocations.length;
  setReferencesState({ query, groups, totalCount, visible: true });
  setActiveBottomTab("references" as any);
  setUIState("bottomPanelVisible", true);
}

export function hideReferences(): void {
  setReferencesState("visible", false);
}

export function showCodeActions(actions: CodeActionItem[], pos: number): void {
  setReferencesState({ codeActions: actions, codeActionsPos: pos, codeActionsVisible: true });
}

export function hideCodeActions(): void {
  setReferencesState("codeActionsVisible", false);
}
