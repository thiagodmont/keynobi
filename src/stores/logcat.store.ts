import { createStore, produce } from "solid-js/store";
import type { LogcatEntry } from "@/lib/tauri-api";

interface LogcatStore {
  entries: LogcatEntry[];
  streaming: boolean;
  crashIndicesFull: number[];
  ringBufferTotal: number | null;
}

export const [logcatState, setLogcatState] = createStore<LogcatStore>({
  entries: [],
  streaming: false,
  crashIndicesFull: [],
  ringBufferTotal: null,
});

export function computeCrashIndices(entries: LogcatEntry[]): number[] {
  return entries.reduce<number[]>((acc, entry, index) => {
    if (entry.isCrash) acc.push(index);
    return acc;
  }, []);
}

function replaceCrashIndices(target: number[], entries: LogcatEntry[]): void {
  target.splice(0, target.length);
  entries.forEach((entry, index) => {
    if (entry.isCrash) target.push(index);
  });
}

export function appendEntriesIncremental(
  entries: LogcatEntry[],
  crashIndices: number[],
  incomingEntries: LogcatEntry[],
  cap: number
): number {
  const safeCap = Math.max(0, Math.floor(cap));
  const baseLen = entries.length;

  for (const entry of incomingEntries) {
    entries.push(entry);
  }

  const dropped = Math.max(0, entries.length - safeCap);
  if (dropped > 0) {
    entries.splice(0, dropped);
    replaceCrashIndices(crashIndices, entries);
    return dropped;
  }

  incomingEntries.forEach((entry, index) => {
    if (entry.isCrash) crashIndices.push(baseLen + index);
  });

  return 0;
}

export function replaceLogcatEntries(entries: LogcatEntry[]): void {
  setLogcatState("entries", entries);
  setLogcatState("crashIndicesFull", computeCrashIndices(entries));
}

export function clearLogcatEntries(): void {
  setLogcatState("entries", []);
  setLogcatState("crashIndicesFull", []);
}

export function appendLogcatEntries(entries: LogcatEntry[], cap: number): number {
  let dropped = 0;
  setLogcatState(
    produce((state) => {
      dropped = appendEntriesIncremental(state.entries, state.crashIndicesFull, entries, cap);
    })
  );
  return dropped;
}

export function setLogcatStreaming(streaming: boolean): void {
  setLogcatState("streaming", streaming);
}

export function setLogcatRingBufferTotal(total: number | null): void {
  setLogcatState("ringBufferTotal", total);
}
