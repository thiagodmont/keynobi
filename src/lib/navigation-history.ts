export interface NavigationEntry {
  path: string;
  line: number;
  col: number;
}

const MAX_ENTRIES = 50;

let history: NavigationEntry[] = [];
let currentIndex = -1;

export function pushNavigation(entry: NavigationEntry): void {
  // Remove forward history when navigating from middle
  if (currentIndex < history.length - 1) {
    history = history.slice(0, currentIndex + 1);
  }

  // Deduplicate: don't push if identical to current
  if (history.length > 0) {
    const last = history[history.length - 1];
    if (last.path === entry.path && last.line === entry.line && last.col === entry.col) {
      return;
    }
  }

  history.push(entry);

  if (history.length > MAX_ENTRIES) {
    history = history.slice(history.length - MAX_ENTRIES);
  }

  currentIndex = history.length - 1;
}

export function navigateBack(): NavigationEntry | null {
  if (currentIndex <= 0) return null;
  currentIndex--;
  return history[currentIndex];
}

export function navigateForward(): NavigationEntry | null {
  if (currentIndex >= history.length - 1) return null;
  currentIndex++;
  return history[currentIndex];
}

export function canGoBack(): boolean {
  return currentIndex > 0;
}

export function canGoForward(): boolean {
  return currentIndex < history.length - 1;
}

export function getHistory(): NavigationEntry[] {
  return [...history];
}

export function getCurrentIndex(): number {
  return currentIndex;
}

export function clearHistory(): void {
  history = [];
  currentIndex = -1;
}
