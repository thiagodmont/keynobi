import type { Meta, StoryObj } from "storybook-solidjs-vite";
import { Spinner } from "./Spinner";
import "../design-system.stories.css";

const meta = {
  title: "Design System/Components/Spinner",
  component: Spinner,
  tags: ["autodocs"],
} satisfies Meta<typeof Spinner>;

export default meta;
type Story = StoryObj;

export const Sizes: Story = {
  render: () => (
    <div class="dsStack">
      <Spinner size="sm" />
      <Spinner size="md" />
      <Spinner size="lg" />
    </div>
  ),
};
