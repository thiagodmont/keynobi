import { describe, it, expect } from "vitest";
import { render, screen } from "@solidjs/testing-library";
import { EmptyState } from "./EmptyState";

describe("EmptyState", () => {
  it("renders the title", () => {
    render(() => <EmptyState icon="folder" title="No files" />);
    expect(screen.getByText("No files")).not.toBeNull();
  });

  it("renders description when provided", () => {
    render(() => (
      <EmptyState icon="folder" title="No files" description="Open a folder to get started." />
    ));
    expect(screen.getByText("Open a folder to get started.")).not.toBeNull();
  });

  it("does not render description when absent", () => {
    render(() => <EmptyState icon="folder" title="No files" />);
    expect(screen.queryByText("Open a folder to get started.")).toBeNull();
  });

  it("renders action slot", () => {
    const { container } = render(() => (
      <EmptyState icon="folder" title="No files" action={<button data-testid="act">Open</button>} />
    ));
    expect(container.querySelector("[data-testid='act']")).not.toBeNull();
  });

  it("does not render action wrapper when action is absent", () => {
    const { container } = render(() => <EmptyState icon="folder" title="No files" />);
    expect(container.querySelector("[data-testid='act']")).toBeNull();
  });

  it("passes class prop through to root", () => {
    const { container } = render(() => (
      <EmptyState icon="folder" title="T" class="my-empty" />
    ));
    expect(container.firstElementChild!.classList.contains("my-empty")).toBe(true);
  });
});
