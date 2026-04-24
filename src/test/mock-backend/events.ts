import type { UnlistenFn } from "@tauri-apps/api/event";

type WrappedHandler = (payload: unknown) => void;
const registry = new Map<string, Set<WrappedHandler>>();

export function handleListen<T>(
  event: string,
  handler: (e: { payload: T }) => void
): Promise<UnlistenFn> {
  if (!registry.has(event)) registry.set(event, new Set());
  const wrapped: WrappedHandler = (p) => handler({ payload: p as T });
  registry.get(event)!.add(wrapped);
  return Promise.resolve(() => {
    registry.get(event)?.delete(wrapped);
  });
}

export function handleEmit(): Promise<void> {
  return Promise.resolve();
}

export function triggerEvent(event: string, payload: unknown): void {
  registry.get(event)?.forEach((h) => h(payload));
}
