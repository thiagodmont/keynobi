import type { Meta, StoryObj } from "storybook-solidjs-vite";
import { StatusDot } from "./StatusDot";
import "../design-system.stories.css";

const meta = {
  title: "Design System/Components/StatusDot",
  component: StatusDot,
  tags: ["autodocs"],
} satisfies Meta<typeof StatusDot>;

export default meta;
type Story = StoryObj<typeof meta>;

export const States: Story = {
  render: () => (
    <div class="dsStack">
      <StatusDot status="ok" />
      <StatusDot status="warning" />
      <StatusDot status="error" />
      <StatusDot status="active" />
      <StatusDot status="idle" />
      <StatusDot status="ok" size="sm" />
    </div>
  ),
};
