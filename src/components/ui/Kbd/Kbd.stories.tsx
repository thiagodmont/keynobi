import type { Meta, StoryObj } from "storybook-solidjs-vite";
import { Kbd } from "./Kbd";
import "../design-system.stories.css";

const meta = {
  title: "Design System/Components/Kbd",
  component: Kbd,
  tags: ["autodocs"],
} satisfies Meta<typeof Kbd>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Shortcut: Story = {
  render: () => (
    <div class="dsStack">
      <Kbd>Cmd</Kbd>
      <Kbd>Shift</Kbd>
      <Kbd>P</Kbd>
    </div>
  ),
};
