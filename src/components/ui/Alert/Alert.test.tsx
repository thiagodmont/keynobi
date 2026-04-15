import { describe, it, expect, vi } from "vitest";
import { render, fireEvent, screen } from "@solidjs/testing-library";
import { Alert } from "./Alert";

describe("Alert", () => {
  it("renders children", () => {
    render(() => <Alert variant="info">Alert body</Alert>);
    expect(screen.getByText("Alert body")).not.toBeNull();
  });

  it("renders title when provided", () => {
    render(() => <Alert variant="warning" title="Watch out">msg</Alert>);
    expect(screen.getByText("Watch out")).not.toBeNull();
  });

  it("does not render title element when title is absent", () => {
    render(() => <Alert variant="info">msg</Alert>);
    expect(screen.queryByText("Watch out")).toBeNull();
  });

  it("renders close button when dismissible", () => {
    const { container } = render(() => (
      <Alert variant="info" dismissible onDismiss={vi.fn()}>msg</Alert>
    ));
    expect(container.querySelector("button")).not.toBeNull();
  });

  it("does not render close button when not dismissible", () => {
    const { container } = render(() => <Alert variant="info">msg</Alert>);
    expect(container.querySelector("button")).toBeNull();
  });

  it("calls onDismiss when close button is clicked", () => {
    const fn = vi.fn();
    const { container } = render(() => (
      <Alert variant="error" dismissible onDismiss={fn}>msg</Alert>
    ));
    fireEvent.click(container.querySelector("button")!);
    expect(fn).toHaveBeenCalledOnce();
  });

  it("renders action slot", () => {
    const { container } = render(() => (
      <Alert variant="info" action={<button data-testid="act">Retry</button>}>msg</Alert>
    ));
    expect(container.querySelector("[data-testid='act']")).not.toBeNull();
  });

  it("passes class prop through to root", () => {
    const { container } = render(() => (
      <Alert variant="success" class="my-alert">msg</Alert>
    ));
    expect(container.firstElementChild!.classList.contains("my-alert")).toBe(true);
  });
});
