import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, fireEvent, screen } from "@solidjs/testing-library";
import { CopyableText } from "./CopyableText";

const mockClipboard = {
  writeText: vi.fn().mockResolvedValue(undefined),
};

beforeEach(() => {
  Object.defineProperty(navigator, "clipboard", {
    value: mockClipboard,
    writable: true,
    configurable: true,
  });
  mockClipboard.writeText.mockClear();
});

describe("CopyableText", () => {
  it("renders the text", () => {
    render(() => <CopyableText text="hello" />);
    expect(screen.getByText("hello")).not.toBeNull();
  });

  it("renders a copy button", () => {
    const { container } = render(() => <CopyableText text="hello" />);
    expect(container.querySelector("button")).not.toBeNull();
  });

  it("calls navigator.clipboard.writeText with the text on click", () => {
    const { container } = render(() => <CopyableText text="hello" />);
    fireEvent.click(container.querySelector("button")!);
    expect(mockClipboard.writeText).toHaveBeenCalledWith("hello");
  });

  it("in iconOnly mode does not render the text span", () => {
    render(() => <CopyableText text="hello" iconOnly />);
    expect(screen.queryByText("hello")).toBeNull();
  });

  it("in iconOnly mode still renders a button", () => {
    const { container } = render(() => <CopyableText text="hello" iconOnly />);
    expect(container.querySelector("button")).not.toBeNull();
  });

  it("passes class prop through to root", () => {
    const { container } = render(() => <CopyableText text="hi" class="my-ct" />);
    expect(container.firstElementChild!.classList.contains("my-ct")).toBe(true);
  });
});
