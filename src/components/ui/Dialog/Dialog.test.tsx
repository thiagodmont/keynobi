import { afterEach, beforeEach, describe, it, expect, vi } from "vitest";
import { render, fireEvent, screen } from "@solidjs/testing-library";
import { DialogHost, showDialog } from "./Dialog";
import { resetDialogHostForTests } from "./Dialog.test-utils";

describe("Dialog", () => {
  beforeEach(() => {
    resetDialogHostForTests();
  });
  it("renders nothing when no dialog is pending", () => {
    const { container } = render(() => <DialogHost />);
    expect(container.firstChild).toBeNull();
  });

  it("renders dialog title and message after showDialog", async () => {
    render(() => <DialogHost />);
    showDialog({
      title: "Confirm",
      message: "Are you sure?",
      buttons: [{ label: "OK", value: "ok", style: "primary" }],
    });
    expect(screen.getByText("Confirm")).not.toBeNull();
    expect(screen.getByText("Are you sure?")).not.toBeNull();
  });

  it("resolves promise with button value on click", async () => {
    render(() => <DialogHost />);
    const promise = showDialog({
      title: "Test",
      message: "msg",
      buttons: [{ label: "Yes", value: "yes", style: "primary" }],
    });
    fireEvent.click(screen.getByText("Yes"));
    const result = await promise;
    expect(result).toBe("yes");
  });

  it("closes dialog after button click", async () => {
    render(() => <DialogHost />);
    const p = showDialog({
      title: "T",
      message: "m",
      buttons: [{ label: "OK", value: "ok", style: "primary" }],
    });
    fireEvent.click(screen.getByText("OK"));
    await p;
    expect(screen.queryByText("T")).toBeNull();
  });

  it("resolves with 'cancel' when backdrop is clicked", async () => {
    render(() => <DialogHost />);
    const promise = showDialog({
      title: "T",
      message: "m",
      buttons: [{ label: "OK", value: "ok", style: "primary" }],
    });
    // backdrop is the outermost div rendered by Portal
    const backdrop = document.querySelector("[data-testid='dialog-backdrop']")!;
    fireEvent.click(backdrop);
    const result = await promise;
    expect(result).toBe("cancel");
  });

  it("queues a second showDialog and resolves both in order", async () => {
    render(() => <DialogHost />);
    const p1 = showDialog({
      title: "First",
      message: "m1",
      buttons: [{ label: "Next", value: "next", style: "primary" }],
    });
    const p2 = showDialog({
      title: "Second",
      message: "m2",
      buttons: [{ label: "Done", value: "done", style: "primary" }],
    });
    expect(screen.getByText("First")).not.toBeNull();
    expect(screen.queryByText("Second")).toBeNull();
    expect(document.querySelectorAll("[data-testid='dialog-backdrop']").length).toBe(1);
    fireEvent.click(screen.getByText("Next"));
    expect(await p1).toBe("next");
    expect(screen.getByText("Second")).not.toBeNull();
    fireEvent.click(screen.getByText("Done"));
    expect(await p2).toBe("done");
  });
  describe("keyboard and focus", () => {
    let trigger: HTMLButtonElement;

    beforeEach(() => {
      trigger = document.createElement("button");
      trigger.textContent = "Trigger";
      document.body.appendChild(trigger);
      trigger.focus();
    });

    afterEach(() => {
      trigger.remove();
    });

    function openTwoButtonDialog() {
      render(() => <DialogHost />);
      return showDialog({
        title: "Delete?",
        message: "m",
        buttons: [
          { label: "Delete", value: "delete", style: "danger" },
          { label: "Cancel", value: "cancel", style: "secondary" },
        ],
      });
    }

    it("moves focus to the first button on open", () => {
      void openTwoButtonDialog();
      expect(document.activeElement).toBe(screen.getByText("Cancel"));
    });

    it("focuses the dialog container when it has no buttons", () => {
      render(() => <DialogHost />);
      void showDialog({ title: "T", message: "m", buttons: [] });
      expect(document.activeElement).toBe(screen.getByRole("dialog"));
    });

    it("resolves with 'cancel' on Escape without reaching document listeners", async () => {
      const documentListener = vi.fn();
      document.addEventListener("keydown", documentListener);
      try {
        const promise = openTwoButtonDialog();
        fireEvent.keyDown(document.activeElement!, { key: "Escape" });
        expect(await promise).toBe("cancel");
        expect(documentListener).not.toHaveBeenCalled();
        expect(screen.queryByRole("dialog")).toBeNull();
      } finally {
        document.removeEventListener("keydown", documentListener);
      }
    });

    it("wraps Tab from the last button to the first", () => {
      void openTwoButtonDialog();
      screen.getByText("Delete").focus();
      fireEvent.keyDown(document.activeElement!, { key: "Tab" });
      expect(document.activeElement).toBe(screen.getByText("Cancel"));
    });

    it("wraps Shift+Tab from the first button to the last", () => {
      void openTwoButtonDialog();
      fireEvent.keyDown(document.activeElement!, { key: "Tab", shiftKey: true });
      expect(document.activeElement).toBe(screen.getByText("Delete"));
    });

    it("returns focus to the previously focused element on close", async () => {
      const promise = openTwoButtonDialog();
      expect(document.activeElement).not.toBe(trigger);
      fireEvent.click(screen.getByText("Delete"));
      expect(await promise).toBe("delete");
      expect(document.activeElement).toBe(trigger);
    });

    it("returns focus to the original element after queued dialogs close", async () => {
      render(() => <DialogHost />);
      const p1 = showDialog({
        title: "First",
        message: "m1",
        buttons: [{ label: "Next", value: "next", style: "primary" }],
      });
      const p2 = showDialog({
        title: "Second",
        message: "m2",
        buttons: [{ label: "Done", value: "done", style: "primary" }],
      });
      fireEvent.keyDown(document.activeElement!, { key: "Escape" });
      expect(await p1).toBe("cancel");
      expect(document.activeElement).toBe(screen.getByText("Done"));
      fireEvent.click(screen.getByText("Done"));
      expect(await p2).toBe("done");
      expect(document.activeElement).toBe(trigger);
    });
  });
});
