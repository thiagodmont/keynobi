import { describe, it, expect, vi } from "vitest";
import { render, fireEvent, screen } from "@solidjs/testing-library";
import { TagInput } from "./TagInput";

describe("TagInput", () => {
  it("renders existing tags", () => {
    const { container } = render(() => (
      <TagInput tags={["foo", "bar"]} onChange={vi.fn()} />
    ));
    expect(container.textContent).toContain("foo");
    expect(container.textContent).toContain("bar");
  });

  it("adds a tag on Enter key", () => {
    const fn = vi.fn();
    render(() => <TagInput tags={[]} onChange={fn} />);
    const input = screen.getByRole("textbox");
    fireEvent.input(input, { target: { value: "newtag" } });
    fireEvent.keyDown(input, { key: "Enter" });
    expect(fn).toHaveBeenCalledWith(["newtag"]);
  });

  it("adds a tag via Add button", () => {
    const fn = vi.fn();
    render(() => <TagInput tags={[]} onChange={fn} />);
    fireEvent.input(screen.getByRole("textbox"), { target: { value: "newtag" } });
    fireEvent.click(screen.getByRole("button", { name: "Add" }));
    expect(fn).toHaveBeenCalledWith(["newtag"]);
  });

  it("clears the input after adding a tag", () => {
    const fn = vi.fn();
    render(() => <TagInput tags={[]} onChange={fn} />);
    const input = screen.getByRole("textbox");
    fireEvent.input(input, { target: { value: "newtag" } });
    fireEvent.keyDown(input, { key: "Enter" });
    expect((input as HTMLInputElement).value).toBe("");
  });

  it("removes a tag when remove button is clicked", () => {
    const fn = vi.fn();
    render(() => <TagInput tags={["foo", "bar"]} onChange={fn} />);
    fireEvent.click(screen.getByRole("button", { name: "Remove foo" }));
    expect(fn).toHaveBeenCalledWith(["bar"]);
  });

  it("does not add a duplicate tag", () => {
    const fn = vi.fn();
    render(() => <TagInput tags={["foo"]} onChange={fn} />);
    fireEvent.input(screen.getByRole("textbox"), { target: { value: "foo" } });
    fireEvent.keyDown(screen.getByRole("textbox"), { key: "Enter" });
    expect(fn).not.toHaveBeenCalled();
  });

  it("does not add a tag when max is reached", () => {
    const fn = vi.fn();
    render(() => <TagInput tags={["a", "b"]} onChange={fn} max={2} />);
    fireEvent.input(screen.getByRole("textbox"), { target: { value: "c" } });
    fireEvent.keyDown(screen.getByRole("textbox"), { key: "Enter" });
    expect(fn).not.toHaveBeenCalled();
  });

  it("passes class prop through", () => {
    const { container } = render(() => (
      <TagInput tags={[]} onChange={vi.fn()} class="my-taginput" />
    ));
    expect(container.firstElementChild!.classList.contains("my-taginput")).toBe(true);
  });
});
