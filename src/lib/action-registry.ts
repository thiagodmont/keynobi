export type ActionCategory =
  | "File"
  | "Edit"
  | "View"
  | "Navigate"
  | "Search"
  | "Build"
  | "Debug"
  | "LSP"
  | "General";

export interface Action {
  id: string;
  label: string;
  category: ActionCategory;
  shortcut?: string;
  icon?: string;
  action: () => void;
}

const actions = new Map<string, Action>();

export function registerAction(action: Action): void {
  actions.set(action.id, action);
}

export function unregisterAction(id: string): void {
  actions.delete(id);
}

export function getAction(id: string): Action | undefined {
  return actions.get(id);
}

export function getActions(): Action[] {
  return Array.from(actions.values());
}

export function getActionsByCategory(category: ActionCategory): Action[] {
  return getActions().filter((a) => a.category === category);
}

export function executeAction(id: string): boolean {
  const action = actions.get(id);
  if (action) {
    action.action();
    return true;
  }
  return false;
}

export function searchActions(query: string): Action[] {
  if (!query.trim()) return getActions();

  const lower = query.toLowerCase();
  return getActions()
    .filter(
      (a) =>
        a.label.toLowerCase().includes(lower) ||
        a.id.toLowerCase().includes(lower) ||
        a.category.toLowerCase().includes(lower)
    )
    .sort((a, b) => {
      const aStarts = a.label.toLowerCase().startsWith(lower) ? 0 : 1;
      const bStarts = b.label.toLowerCase().startsWith(lower) ? 0 : 1;
      if (aStarts !== bStarts) return aStarts - bStarts;
      return a.label.localeCompare(b.label);
    });
}

export function clearActions(): void {
  actions.clear();
}
