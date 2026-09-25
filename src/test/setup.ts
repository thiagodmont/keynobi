/**
 * Global test setup.
 *
 * Mocks the Tauri API surface so unit tests run in jsdom without a real
 * Tauri runtime. Components and stores that call `invoke()` will get the
 * mock instead of crashing.
 */

// Every IPC call a test makes must be stubbed (`vi.mocked(invoke)...`). An
// unstubbed call rejects and fails the test, so a missing stub cannot pass
// silently on `undefined`.
const unstubbedInvokes = vi.hoisted(() => [] as string[]);

afterEach(() => {
  const commands = unstubbedInvokes.splice(0);
  if (commands.length > 0) {
    throw new Error(
      `Unstubbed IPC call(s): ${[...new Set(commands)].join(", ")}. ` +
        "Stub them with vi.mocked(invoke).mockImplementation(...)."
    );
  }
});

// Mock @tauri-apps/api/core
vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn((command: string) => {
    unstubbedInvokes.push(command);
    return Promise.reject(new Error(`Unstubbed IPC call: ${command}`));
  }),
  Channel: class MockChannel<T = unknown> {
    onmessage: ((message: T) => void) | null = null;
  },
}));

// Mock @tauri-apps/api/event
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn().mockResolvedValue(() => {}),
  emit: vi.fn().mockResolvedValue(undefined),
}));

// Mock @tauri-apps/api/window
vi.mock("@tauri-apps/api/window", () => ({
  getCurrentWindow: vi.fn(() => ({
    startDragging: vi.fn().mockResolvedValue(undefined),
    isAlwaysOnTop: vi.fn().mockResolvedValue(false),
    setAlwaysOnTop: vi.fn().mockResolvedValue(undefined),
    setTitle: vi.fn().mockResolvedValue(undefined),
  })),
}));

// Mock @tauri-apps/plugin-dialog
vi.mock("@tauri-apps/plugin-dialog", () => ({
  open: vi.fn().mockResolvedValue(null),
}));

// Mock @tauri-apps/plugin-opener
vi.mock("@tauri-apps/plugin-opener", () => ({
  openUrl: vi.fn().mockResolvedValue(undefined),
}));
