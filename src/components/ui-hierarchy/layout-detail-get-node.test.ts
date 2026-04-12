import { describe, expect, it } from "vitest";
import type { UiNode } from "@/bindings";
import { layoutDetailGetNode } from "./layout-detail-get-node";

const minimalNode = (className: string): UiNode => ({
  class: className,
  resourceId: "",
  text: "",
  contentDesc: "",
  package: "",
  bounds: "[0,0][1,1]",
  clickable: false,
  enabled: true,
  focusable: false,
  focused: false,
  scrollable: false,
  longClickable: false,
  password: false,
  checkable: false,
  checked: false,
  editable: false,
  selected: false,
  isComposeHeuristic: false,
  children: [],
});

describe("layoutDetailGetNode", () => {
  it("returns the current node from getSelected", () => {
    const a = minimalNode("A");
    const b = minimalNode("B");
    let cur: UiNode | null = a;
    const get = layoutDetailGetNode(() => cur);
    expect(get().class).toBe("A");
    cur = b;
    expect(get().class).toBe("B");
  });

  it("throws when getSelected returns null (panel must not mount without selection)", () => {
    const get = layoutDetailGetNode(() => null);
    expect(() => get()).toThrow(/expected a selected node/);
  });
});
