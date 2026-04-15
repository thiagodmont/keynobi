import { describe, it, expect, beforeEach } from "vitest";
import { render, screen } from "@solidjs/testing-library";
import { ToastContainer, showToast, dismissToast, toasts } from "./Toast";

beforeEach(() => {
  // Clear all toasts before each test
  toasts().forEach((t) => dismissToast(t.id));
});

describe("Toast", () => {
  it("renders nothing when no toasts", () => {
    const { container } = render(() => <ToastContainer />);
    expect(container.querySelector("[role='status']")).toBeNull();
  });

  it("renders a toast after showToast", () => {
    render(() => <ToastContainer />);
    showToast("Hello", "info");
    expect(screen.getByText("Hello")).not.toBeNull();
  });

  it("renders error toast with role=alert", () => {
    render(() => <ToastContainer />);
    showToast("Oops", "error");
    expect(screen.getByRole("alert")).not.toBeNull();
  });

  it("renders info toast with role=status", () => {
    render(() => <ToastContainer />);
    showToast("Done", "info");
    expect(screen.getByRole("status")).not.toBeNull();
  });

  it("dismisses toast when close button is clicked", () => {
    render(() => <ToastContainer />);
    showToast("Bye", "info");
    const btn = screen.getByRole("button", { name: "Dismiss" });
    btn.click();
    expect(screen.queryByText("Bye")).toBeNull();
  });

  it("dismissToast removes the toast", () => {
    render(() => <ToastContainer />);
    showToast("Temp", "info");
    const id = toasts()[0].id;
    dismissToast(id);
    expect(screen.queryByText("Temp")).toBeNull();
  });

  it("toasts signal updates after showToast", () => {
    showToast("A", "info");
    expect(toasts().length).toBeGreaterThanOrEqual(1);
    expect(toasts().some((t) => t.message === "A")).toBe(true);
  });
});
