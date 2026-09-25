import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen } from "@solidjs/testing-library";
import { Show, createSignal } from "solid-js";
import { MenuListItem } from "@/components/ui/MenuList";
import { ContextMenu } from "./ContextMenu";

function renderMenu() {
  const rename = vi.fn();
  const remove = vi.fn();
  const [open, setOpen] = createSignal(false);
  render(() => (
    <>
      <button onClick={() => setOpen(true)}>MockProject</button>
      <button>Elsewhere</button>
      <Show when={open()}>
        <ContextMenu x={20} y={30} label="Actions for MockProject" onClose={() => setOpen(false)}>
          <MenuListItem onClick={rename}>Rename…</MenuListItem>
          <MenuListItem onClick={remove}>Remove from List</MenuListItem>
        </ContextMenu>
      </Show>
    </>
  ));
  const trigger = screen.getByRole("button", { name: "MockProject" });
  trigger.focus();
  fireEvent.click(trigger);
  return { trigger, rename, remove, isOpen: open };
}

describe("ContextMenu", () => {
  afterEach(cleanup);

  it("is a named menu and focuses its first item", () => {
    renderMenu();
    expect(screen.getByRole("menu", { name: "Actions for MockProject" })).not.toBeNull();
    expect(document.activeElement?.textContent).toBe("Rename…");
  });

  it("moves between items with the arrow keys, Home, and End", () => {
    renderMenu();
    const [rename, remove] = screen.getAllByRole("menuitem");

    fireEvent.keyDown(rename, { key: "ArrowDown" });
    expect(document.activeElement).toBe(remove);
    fireEvent.keyDown(remove, { key: "ArrowDown" });
    expect(document.activeElement).toBe(rename);
    fireEvent.keyDown(rename, { key: "ArrowUp" });
    expect(document.activeElement).toBe(remove);
    fireEvent.keyDown(remove, { key: "Home" });
    expect(document.activeElement).toBe(rename);
    fireEvent.keyDown(rename, { key: "End" });
    expect(document.activeElement).toBe(remove);
  });

  it("closes on Escape without reaching document listeners, and returns focus", () => {
    const documentListener = vi.fn();
    document.addEventListener("keydown", documentListener);
    const { trigger, isOpen } = renderMenu();

    fireEvent.keyDown(screen.getAllByRole("menuitem")[0], { key: "Escape" });

    expect(isOpen()).toBe(false);
    expect(screen.queryByRole("menu")).toBeNull();
    expect(document.activeElement).toBe(trigger);
    expect(documentListener).not.toHaveBeenCalled();
    document.removeEventListener("keydown", documentListener);
  });

  it("closes on Tab and returns focus", () => {
    const { trigger, isOpen } = renderMenu();
    fireEvent.keyDown(screen.getAllByRole("menuitem")[0], { key: "Tab" });
    expect(isOpen()).toBe(false);
    expect(document.activeElement).toBe(trigger);
  });

  it("runs an item with Enter", () => {
    const { rename } = renderMenu();
    fireEvent.keyDown(screen.getAllByRole("menuitem")[0], { key: "Enter" });
    expect(rename).toHaveBeenCalledOnce();
  });

  it("closes when the pointer goes down outside it", () => {
    const { isOpen } = renderMenu();
    fireEvent.mouseDown(screen.getByRole("button", { name: "Elsewhere" }));
    expect(isOpen()).toBe(false);
  });
});
