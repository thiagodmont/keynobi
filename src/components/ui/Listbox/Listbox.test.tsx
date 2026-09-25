import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen } from "@solidjs/testing-library";
import { createSignal } from "solid-js";
import { Listbox } from "./Listbox";

interface Device {
  serial: string;
  name: string;
  online: boolean;
}

const devices: Device[] = [
  { serial: "emulator-5554", name: "Pixel 8", online: true },
  { serial: "R58M", name: "Galaxy S21", online: false },
  { serial: "192.168.1.4:5555", name: "Pixel 6", online: true },
];

function renderList(opts: { selected?: string | null; onContextMenu?: boolean } = {}) {
  const onSelect = vi.fn();
  const onContextMenu = vi.fn();
  const [items, setItems] = createSignal(devices);
  render(() => (
    <Listbox
      label="Connected devices"
      items={items()}
      getKey={(d) => d.serial}
      isSelected={(d) => d.serial === opts.selected}
      isDisabled={(d) => !d.online}
      onSelect={onSelect}
      onContextMenu={opts.onContextMenu === false ? undefined : onContextMenu}
    >
      {(d) => <span>{d().name}</span>}
    </Listbox>
  ));
  return { onSelect, onContextMenu, setItems, options: () => screen.getAllByRole("option") };
}

describe("Listbox", () => {
  afterEach(cleanup);

  it("is a named listbox with one option per item and the selection marked", () => {
    const { options } = renderList({ selected: "R58M" });
    expect(screen.getByRole("listbox", { name: "Connected devices" })).not.toBeNull();
    expect(options().map((o) => o.getAttribute("aria-selected"))).toEqual([
      "false",
      "true",
      "false",
    ]);
    expect(options()[1].getAttribute("aria-disabled")).toBe("true");
  });

  it("has one tab stop: the selected option, else the first", () => {
    const first = renderList({ selected: "192.168.1.4:5555" });
    expect(first.options().map((o) => o.tabIndex)).toEqual([-1, -1, 0]);
    cleanup();

    const none = renderList({ selected: null });
    expect(none.options().map((o) => o.tabIndex)).toEqual([0, -1, -1]);
  });

  it("moves focus with the arrow keys, Home, and End without selecting", () => {
    const { options, onSelect } = renderList();
    const [a, b, c] = options();
    a.focus();

    fireEvent.keyDown(a, { key: "ArrowDown" });
    expect(document.activeElement).toBe(b);
    expect(b.tabIndex).toBe(0);
    expect(a.tabIndex).toBe(-1);

    fireEvent.keyDown(b, { key: "End" });
    expect(document.activeElement).toBe(c);
    fireEvent.keyDown(c, { key: "ArrowDown" });
    expect(document.activeElement).toBe(c);
    fireEvent.keyDown(c, { key: "Home" });
    expect(document.activeElement).toBe(a);
    fireEvent.keyDown(a, { key: "ArrowUp" });
    expect(document.activeElement).toBe(a);

    expect(onSelect).not.toHaveBeenCalled();
  });

  it("selects with Enter, Space, or a click, but not a disabled option", () => {
    const { options, onSelect } = renderList();
    const [a, offline, c] = options();

    fireEvent.keyDown(a, { key: "Enter" });
    fireEvent.keyDown(c, { key: " " });
    fireEvent.click(a);
    fireEvent.keyDown(offline, { key: "Enter" });
    fireEvent.click(offline);

    expect(onSelect.mock.calls.map(([d]) => d.serial)).toEqual([
      "emulator-5554",
      "192.168.1.4:5555",
      "emulator-5554",
    ]);
  });

  it("asks for the option's menu from Shift+F10, the context-menu key, and right-click", () => {
    const { options, onContextMenu } = renderList();
    const [a, , c] = options();

    fireEvent.keyDown(a, { key: "F10", shiftKey: true });
    fireEvent.keyDown(c, { key: "ContextMenu" });
    fireEvent.contextMenu(a, { clientX: 40, clientY: 50 });

    expect(onContextMenu.mock.calls.map(([r]) => r.item.serial)).toEqual([
      "emulator-5554",
      "192.168.1.4:5555",
      "emulator-5554",
    ]);
    expect(onContextMenu.mock.calls[2][0]).toMatchObject({ x: 40, y: 50 });
  });

  it("ignores keys typed into a control inside an option", () => {
    const onSelect = vi.fn();
    render(() => (
      <Listbox
        label="Projects"
        items={["a"]}
        getKey={(k) => k}
        isSelected={() => false}
        onSelect={onSelect}
      >
        {() => <input aria-label="Rename" />}
      </Listbox>
    ));

    fireEvent.keyDown(screen.getByLabelText("Rename"), { key: " " });
    fireEvent.keyDown(screen.getByLabelText("Rename"), { key: "Enter" });

    expect(onSelect).not.toHaveBeenCalled();
  });

  it("keeps the focused option when the list is refreshed with new objects", () => {
    const { options, setItems } = renderList();
    const b = options()[1];
    b.focus();

    setItems(devices.map((d) => ({ ...d, name: `${d.name} (refreshed)` })));

    expect(options()[1]).toBe(b);
    expect(document.activeElement).toBe(b);
    expect(b.textContent).toBe("Galaxy S21 (refreshed)");
  });
});
