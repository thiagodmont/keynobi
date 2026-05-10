import type { Meta, StoryObj } from "storybook-solidjs-vite";
import { VirtualList } from "./VirtualList";
import "../design-system.stories.css";

const meta = {
  title: "Design System/Components/VirtualList",
  component: VirtualList,
  tags: ["autodocs"],
  parameters: {
    docs: {
      description: {
        component:
          "Use VirtualList for high-volume fixed-height rows such as Logcat and build output.",
      },
    },
  },
} satisfies Meta<typeof VirtualList>;

export default meta;
type Story = StoryObj<typeof meta>;

const rows = Array.from({ length: 1000 }, (_, index) => ({
  id: index + 1,
  message: `Log row ${index + 1}: ActivityManager displayed com.example.app`,
}));

export const LogRows: Story = {
  render: () => (
    <VirtualList
      items={rows}
      rowHeight={22}
      overscan={8}
      style={{
        height: "260px",
        border: "1px solid var(--border)",
        "background-color": "var(--bg-secondary)",
      }}
      renderRow={(row) => (
        <div
          style={{
            height: "22px",
            display: "flex",
            "align-items": "center",
            gap: "8px",
            padding: "0 8px",
            "font-family": "var(--font-mono)",
            "font-size": "var(--font-size-ui-sm)",
            color: "var(--text-secondary)",
          }}
        >
          <span style={{ color: "var(--text-muted)" }}>{row.id}</span>
          <span>{row.message}</span>
        </div>
      )}
    />
  ),
};
