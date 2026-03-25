import { describe, it, expect, beforeEach, vi } from "vitest";
import { registerKeybinding, initKeybindings, listKeybindings } from "./keybindings";

// ── Helpers ───────────────────────────────────────────────────────────────────

/**
 * Dispatch a real KeyboardEvent on `document` and return it.
 * jsdom requires a real Event instance — plain objects are rejected.
 * We attach a custom `_mockTarget` property so the listener can use it;
 * since jsdom sets `event.target` to the dispatching element, we patch
 * `initKeybindings` behaviour by dispatching from a real DOM element.
 */
function dispatchKey(
  key: string,
  modifiers: { metaKey?: boolean; ctrlKey?: boolean; shiftKey?: boolean; altKey?: boolean } = {},
  targetTag: "DIV" | "INPUT" | "TEXTAREA" = "DIV"
) {
  const el =
    targetTag === "DIV"
      ? document.body
      : document.createElement(targetTag.toLowerCase());

  if (targetTag !== "DIV") {
    document.body.appendChild(el);
  }

  const event = new KeyboardEvent("keydown", {
    key,
    metaKey: modifiers.metaKey ?? false,
    ctrlKey: modifiers.ctrlKey ?? false,
    shiftKey: modifiers.shiftKey ?? false,
    altKey: modifiers.altKey ?? false,
    bubbles: true,
    cancelable: true,
  });

  el.dispatchEvent(event);

  if (targetTag !== "DIV") {
    document.body.removeChild(el);
  }

  return event;
}

// Clear registry between tests by re-importing. Because keybindings.ts uses a
// module-level array we cannot easily reset it between tests. Instead we verify
// the *behaviour* (action execution and dedup) without relying on exact array
// contents from other tests.

// ── registerKeybinding ────────────────────────────────────────────────────────

describe("registerKeybinding — deduplication by key combo", () => {
  it("registers a binding and makes it retrievable", () => {
    const action = vi.fn();
    registerKeybinding({ key: "x", metaKey: true, action, description: "Test X" });
    const all = listKeybindings();
    expect(all.some((b) => b.description === "Test X")).toBe(true);
  });

  it("replaces an existing binding with the same key combo", () => {
    const first = vi.fn();
    const second = vi.fn();
    registerKeybinding({ key: "q", metaKey: true, action: first, description: "First Q" });
    registerKeybinding({ key: "q", metaKey: true, action: second, description: "Second Q" });

    const all = listKeybindings();
    const matching = all.filter(
      (b) => b.key === "q" && b.metaKey && !b.ctrlKey && !b.shiftKey && !b.altKey
    );
    // Should only have one binding for Cmd+Q
    expect(matching).toHaveLength(1);
    expect(matching[0].description).toBe("Second Q");
  });

  it("does NOT deduplicate bindings with different key combos even if they share a description", () => {
    const a1 = vi.fn();
    const a2 = vi.fn();
    registerKeybinding({ key: "r", metaKey: true, action: a1, description: "Do thing" });
    registerKeybinding({ key: "r", shiftKey: true, action: a2, description: "Do thing" });

    // Both should coexist — different combos
    const all = listKeybindings();
    const rBindings = all.filter((b) => b.key === "r");
    const metaR = rBindings.find((b) => b.metaKey && !b.shiftKey);
    const shiftR = rBindings.find((b) => b.shiftKey && !b.metaKey);
    expect(metaR).toBeDefined();
    expect(shiftR).toBeDefined();
  });
});

// ── initKeybindings — event dispatching ──────────────────────────────────────

// initKeybindings adds a document keydown listener. We call it once per suite;
// calling it multiple times would multiply the listener and inflate call counts.
initKeybindings();

describe("initKeybindings", () => {

  it("calls the action when a matching keydown fires", () => {
    const action = vi.fn();
    registerKeybinding({ key: "k", metaKey: true, action, description: "Test K action" });
    dispatchKey("k", { metaKey: true });
    expect(action).toHaveBeenCalledTimes(1);
  });

  it("does not call the action when a non-matching key fires", () => {
    const action = vi.fn();
    registerKeybinding({ key: "m", metaKey: true, action, description: "Test M action" });
    dispatchKey("n", { metaKey: true });
    expect(action).not.toHaveBeenCalled();
  });

  it("does not call the action when modifiers differ", () => {
    const action = vi.fn();
    registerKeybinding({
      key: "p",
      metaKey: true,
      shiftKey: true,
      action,
      description: "Meta+Shift+P",
    });
    // Fire without shift — should NOT match
    dispatchKey("p", { metaKey: true, shiftKey: false });
    expect(action).not.toHaveBeenCalled();
  });

  it("skips input elements by default", () => {
    const action = vi.fn();
    registerKeybinding({ key: "z", metaKey: true, action, description: "Undo Z" });
    dispatchKey("z", { metaKey: true }, "INPUT");
    expect(action).not.toHaveBeenCalled();
  });

  it("fires global bindings even inside input elements", () => {
    const action = vi.fn();
    registerKeybinding({
      key: "g",
      metaKey: true,
      action,
      description: "Global G",
      context: "global",
    });
    dispatchKey("g", { metaKey: true }, "INPUT");
    expect(action).toHaveBeenCalledTimes(1);
  });
});
