import type { Meta, StoryObj } from "storybook-solidjs-vite";
import { Button } from "@/components/ui/Button";
import { Tooltip } from "./Tooltip";
import "../design-system.stories.css";

const meta = {
  title: "Design System/Components/Tooltip",
  component: Tooltip,
  tags: ["autodocs"],
} satisfies Meta<typeof Tooltip>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Positions: Story = {
  render: () => (
    <div class="dsStack">
      <Tooltip content="Start streaming logs" delay={0} position="top">
        <Button variant="outline" size="sm">
          Top
        </Button>
      </Tooltip>
      <Tooltip content="Stop streaming logs" delay={0} position="bottom">
        <Button variant="outline" size="sm">
          Bottom
        </Button>
      </Tooltip>
      <Tooltip content="Disabled tooltip" disabled>
        <Button variant="outline" size="sm">
          Disabled
        </Button>
      </Tooltip>
    </div>
  ),
};
