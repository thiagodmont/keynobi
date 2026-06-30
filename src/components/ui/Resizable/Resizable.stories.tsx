import type { Meta, StoryObj } from "storybook-solidjs-vite";
import { createSignal } from "solid-js";
import { Resizable } from "./Resizable";
import "../design-system.stories.css";

const meta = {
  title: "Design System/Components/Resizable",
  component: Resizable,
  tags: ["autodocs"],
} satisfies Meta<typeof Resizable>;

export default meta;
type Story = StoryObj;

export const Handles: Story = {
  render: () => {
    const [width, setWidth] = createSignal(220);

    return (
      <div style={{ display: "flex", height: "180px", width: "420px" }}>
        <div
          style={{
            width: `${width()}px`,
            padding: "12px",
            background: "var(--bg-secondary)",
            border: "1px solid var(--border)",
          }}
        >
          Resizable panel
        </div>
        <Resizable
          direction="horizontal"
          onResize={(delta) => setWidth((value) => Math.max(120, value + delta))}
          onReset={() => setWidth(220)}
        />
        <div style={{ flex: "1", padding: "12px", color: "var(--text-muted)" }}>Content</div>
      </div>
    );
  },
};
