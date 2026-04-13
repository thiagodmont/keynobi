import { describe, it, expect, vi } from "vitest";
import { fireEvent, render } from "@solidjs/testing-library";
import type { UiNode } from "@/bindings";
import { LayoutWireframe } from "./LayoutWireframe";

/** Single full-screen node — enough for wireframe hit-test / pan cursor behavior. */
const MINIMAL_ROOT: UiNode = {
  class: "android.widget.FrameLayout",
  resourceId: "",
  text: "",
  contentDesc: "",
  package: "p",
  bounds: "[0,0][400,800]",
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
};

describe("LayoutWireframe", () => {
  it("svg cursor becomes grabbing while dragging and reverts after mouseup", () => {
    const onSelectPath = vi.fn();
    const { container } = render(() => (
      <LayoutWireframe root={MINIMAL_ROOT} selectedPath={null} onSelectPath={onSelectPath} />
    ));
    const svg = container.querySelector('svg[role="img"]') as SVGSVGElement | null;
    expect(svg).not.toBeNull();
    expect(svg!.style.cursor).toBe("crosshair");

    fireEvent.mouseDown(svg!, { button: 0, clientX: 50, clientY: 50 });
    expect(svg!.style.cursor).toBe("grabbing");

    // Movement ≥4 skips the click → hitTest path (jsdom SVG has no createSVGPoint).
    fireEvent.mouseUp(svg!, { button: 0, clientX: 60, clientY: 50 });
    expect(svg!.style.cursor).toBe("crosshair");
  });
});
