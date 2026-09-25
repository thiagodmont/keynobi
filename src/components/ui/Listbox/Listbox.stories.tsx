import type { Meta, StoryObj } from "storybook-solidjs-vite";
import { createSignal } from "solid-js";
import { Listbox } from "./Listbox";
import "../design-system.stories.css";

const meta = {
  title: "Design System/Components/Listbox",
  component: Listbox,
  tags: ["autodocs"],
  parameters: {
    docs: {
      description: {
        component:
          "Single-select list with one tab stop. Up/Down, Home, and End move focus; Enter or Space selects; Shift+F10 or the context-menu key asks for the option's actions. Use it for sidebars that pick the active project or device.",
      },
    },
  },
} satisfies Meta<typeof Listbox>;

export default meta;
type Story = StoryObj;

interface DeviceRow {
  serial: string;
  name: string;
  detail: string;
  online: boolean;
}

const devices: DeviceRow[] = [
  { serial: "emulator-5554", name: "Pixel 8", detail: "Emulator · API 35", online: true },
  { serial: "R58M", name: "Galaxy S21", detail: "Physical · API 34 · offline", online: false },
  { serial: "192.168.1.4:5555", name: "Pixel 6", detail: "Physical · API 34", online: true },
];

export const Devices: Story = {
  render: () => {
    const [selected, setSelected] = createSignal("emulator-5554");
    return (
      <div class="dsCard" style={{ width: "240px" }}>
        <Listbox
          label="Connected devices"
          items={devices}
          getKey={(d) => d.serial}
          isSelected={(d) => d.serial === selected()}
          isDisabled={(d) => !d.online}
          onSelect={(d) => setSelected(d.serial)}
        >
          {(d) => (
            <div
              style={{
                padding: "6px 8px",
                background: d().serial === selected() ? "var(--accent-bg)" : "transparent",
                "border-radius": "6px",
                opacity: d().online ? "1" : "0.6",
              }}
            >
              <div style={{ "font-size": "12px", color: "var(--text-primary)" }}>{d().name}</div>
              <div style={{ "font-size": "11px", color: "var(--text-secondary)" }}>
                {d().detail}
              </div>
            </div>
          )}
        </Listbox>
      </div>
    );
  },
};
