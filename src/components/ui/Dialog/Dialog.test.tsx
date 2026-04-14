import { beforeEach, describe, it, expect } from "vitest";
import { render, fireEvent, screen } from "@solidjs/testing-library";
import { DialogHost, resetDialogHostForTests, showDialog } from "./Dialog";

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
    fireEvent.click(screen.getByText("Next"));
    expect(await p1).toBe("next");
    expect(screen.getByText("Second")).not.toBeNull();
    fireEvent.click(screen.getByText("Done"));
    expect(await p2).toBe("done");
  });
});
