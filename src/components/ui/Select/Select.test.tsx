import { describe, it, expect, vi } from "vitest";
import { render, fireEvent } from "@solidjs/testing-library";
import { Select } from "./Select";

describe("Select", () => {
  it("renders a select element", () => {
    const { container } = render(() => (
      <Select value="a" options={["a", "b"]} onChange={vi.fn()} />
    ));
    expect(container.querySelector("select")).not.toBeNull();
  });

  it("renders string options", () => {
    const { container } = render(() => (
      <Select value="a" options={["a", "b", "c"]} onChange={vi.fn()} />
    ));
    const opts = container.querySelectorAll("option");
    expect(opts.length).toBe(3);
    expect(opts[0].value).toBe("a");
    expect(opts[1].value).toBe("b");
  });

  it("renders object options with label and value", () => {
    const { container } = render(() => (
      <Select
        value="v1"
        options={[{ label: "Option 1", value: "v1" }, { label: "Option 2", value: "v2" }]}
        onChange={vi.fn()}
      />
    ));
    const opts = container.querySelectorAll("option");
    expect(opts[0].textContent).toBe("Option 1");
    expect(opts[0].value).toBe("v1");
  });

  it("calls onChange with the selected value", () => {
    const fn = vi.fn();
    const { container } = render(() => (
      <Select value="a" options={["a", "b"]} onChange={fn} />
    ));
    fireEvent.change(container.querySelector("select")!, { target: { value: "b" } });
    expect(fn).toHaveBeenCalledWith("b");
  });

  it("renders a disabled placeholder option when placeholder is set", () => {
    const { container } = render(() => (
      <Select value="" options={["a"]} onChange={vi.fn()} placeholder="Choose..." />
    ));
    const first = container.querySelector("option")!;
    expect(first.textContent).toBe("Choose...");
    expect(first.disabled).toBe(true);
  });

  it("is disabled when disabled prop is set", () => {
    const { container } = render(() => (
      <Select value="a" options={["a"]} onChange={vi.fn()} disabled />
    ));
    expect(container.querySelector("select")!.disabled).toBe(true);
  });

  it("passes class prop through", () => {
    const { container } = render(() => (
      <Select value="a" options={["a"]} onChange={vi.fn()} class="my-select" />
    ));
    expect(container.querySelector("select")!.classList.contains("my-select")).toBe(true);
  });
});
