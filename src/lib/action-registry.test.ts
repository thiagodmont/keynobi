import { describe, it, expect, beforeEach, vi } from "vitest";
import {
  registerAction,
  unregisterAction,
  getAction,
  getActions,
  getActionsByCategory,
  executeAction,
  searchActions,
  clearActions,
  type Action,
} from "@/lib/action-registry";

function makeAction(id: string, label: string, category: Action["category"] = "General"): Action {
  return {
    id,
    label,
    category,
    action: vi.fn(),
  };
}

describe("action-registry", () => {
  beforeEach(() => {
    clearActions();
  });

  it("registers and retrieves an action", () => {
    const action = makeAction("test.action", "Test Action");
    registerAction(action);
    expect(getAction("test.action")).toBe(action);
  });

  it("returns undefined for unknown action", () => {
    expect(getAction("nonexistent")).toBeUndefined();
  });

  it("lists all actions", () => {
    registerAction(makeAction("a", "Alpha"));
    registerAction(makeAction("b", "Beta"));
    expect(getActions()).toHaveLength(2);
  });

  it("unregisters an action", () => {
    registerAction(makeAction("a", "Alpha"));
    unregisterAction("a");
    expect(getAction("a")).toBeUndefined();
    expect(getActions()).toHaveLength(0);
  });

  it("filters by category", () => {
    registerAction(makeAction("f1", "Open File", "File"));
    registerAction(makeAction("f2", "Save", "File"));
    registerAction(makeAction("e1", "Undo", "Edit"));
    expect(getActionsByCategory("File")).toHaveLength(2);
    expect(getActionsByCategory("Edit")).toHaveLength(1);
    expect(getActionsByCategory("Navigate")).toHaveLength(0);
  });

  it("executes an action", () => {
    const action = makeAction("test", "Test");
    registerAction(action);
    const result = executeAction("test");
    expect(result).toBe(true);
    expect(action.action).toHaveBeenCalledOnce();
  });

  it("returns false for unknown action execution", () => {
    expect(executeAction("nonexistent")).toBe(false);
  });

  it("searches actions by label", () => {
    registerAction(makeAction("a", "Open File", "File"));
    registerAction(makeAction("b", "Open Folder", "File"));
    registerAction(makeAction("c", "Save File", "File"));

    const results = searchActions("open");
    expect(results).toHaveLength(2);
    expect(results[0].label).toBe("Open File");
    expect(results[1].label).toBe("Open Folder");
  });

  it("searches actions by category", () => {
    registerAction(makeAction("a", "Open File", "File"));
    registerAction(makeAction("b", "Toggle Panel", "View"));

    const results = searchActions("view");
    expect(results).toHaveLength(1);
    expect(results[0].id).toBe("b");
  });

  it("returns all actions for empty search", () => {
    registerAction(makeAction("a", "Alpha"));
    registerAction(makeAction("b", "Beta"));
    expect(searchActions("")).toHaveLength(2);
    expect(searchActions("  ")).toHaveLength(2);
  });

  it("ranks prefix matches higher", () => {
    registerAction(makeAction("a", "Close Tab"));
    registerAction(makeAction("b", "Close All Tabs"));
    registerAction(makeAction("c", "Show Close Dialog"));

    const results = searchActions("close");
    expect(results[0].label).toBe("Close All Tabs");
    expect(results[1].label).toBe("Close Tab");
  });

  it("clears all actions", () => {
    registerAction(makeAction("a", "Alpha"));
    registerAction(makeAction("b", "Beta"));
    clearActions();
    expect(getActions()).toHaveLength(0);
  });
});
