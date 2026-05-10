import type { Meta, StoryObj } from "storybook-solidjs-vite";
import { ProgressBar } from "./ProgressBar";
import "../design-system.stories.css";

const meta = {
  title: "Design System/Components/ProgressBar",
  component: ProgressBar,
  tags: ["autodocs"],
} satisfies Meta<typeof ProgressBar>;

export default meta;
type Story = StoryObj<typeof meta>;

export const States: Story = {
  render: () => (
    <div class="dsColumn" style={{ width: "360px" }}>
      <ProgressBar value={64} />
      <ProgressBar value={42} variant="success" size="md" />
      <ProgressBar value={20} variant="warning" />
      <ProgressBar />
    </div>
  ),
};
