export interface LatestOnlyGuard {
  begin(): number;
  isLatest(token: number): boolean;
  invalidate(): void;
}

export function createLatestOnlyGuard(): LatestOnlyGuard {
  let latest = 0;
  return {
    begin() {
      latest += 1;
      return latest;
    },
    isLatest(token: number) {
      return token === latest;
    },
    invalidate() {
      latest += 1;
    },
  };
}
