import type { Meta, StoryObj } from "storybook-solidjs-vite";
import { Toolbar } from "./Toolbar";
import "../design-system.stories.css";

const meta = {
  title: "Design System/Components/Toolbar",
  component: Toolbar,
  tags: ["autodocs"],
} satisfies Meta<typeof Toolbar>;

export default meta;
type Story = StoryObj;

export const Compact: Story = {
  render: () => (
    <Toolbar
      compact
      items={[
        { id: "run", label: "Run", onClick: () => {} },
        { id: "stop", label: "Stop", onClick: () => {}, disabled: true },
        { id: "divider", label: "Divider", onClick: () => {}, separator: true },
        { id: "logs", label: "Logs", onClick: () => {}, active: true },
      ]}
    />
  ),
};
