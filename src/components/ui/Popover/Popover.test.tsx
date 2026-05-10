import { createSignal } from "solid-js";
import { describe, expect, it } from "vitest";
import { fireEvent, render } from "@solidjs/testing-library";
import { Popover } from "./Popover";

describe("Popover", () => {
  it("renders trigger and panel when open", () => {
    const { getByText } = render(() => (
      <Popover
        open
        onOpenChange={() => {}}
        trigger={(api) => <button onClick={api.toggle}>Open</button>}
      >
        <div>Panel content</div>
      </Popover>
    ));
    expect(getByText("Open")).not.toBeNull();
    expect(getByText("Panel content")).not.toBeNull();
  });

  it("closes through overlay click", () => {
    const Harness = () => {
      const [open, setOpen] = createSignal(true);
      return (
        <Popover
          open={open()}
          onOpenChange={setOpen}
          trigger={(api) => <button onClick={api.toggle}>Open</button>}
        >
          <div>Panel content</div>
        </Popover>
      );
    };
    const { container, queryByText } = render(() => <Harness />);
    fireEvent.click(container.querySelector('[class*="overlay"]')!);
    expect(queryByText("Panel content")).toBeNull();
  });
});
