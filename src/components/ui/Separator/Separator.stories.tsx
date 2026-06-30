import type { Meta, StoryObj } from "storybook-solidjs-vite";
import { Button } from "@/components/ui/Button";
import { Separator } from "./Separator";
import "../design-system.stories.css";

const meta = {
  title: "Design System/Components/Separator",
  component: Separator,
  tags: ["autodocs"],
} satisfies Meta<typeof Separator>;

export default meta;
type Story = StoryObj;

export const Orientations: Story = {
  render: () => (
    <div class="dsColumn">
      <div class="dsStack">
        <Button variant="outline" size="xs">
          Start
        </Button>
        <Separator orientation="vertical" spacing="sm" />
        <Button variant="outline" size="xs">
          Stop
        </Button>
      </div>
      <Separator spacing="md" />
      <span class="dsCode">Horizontal separator</span>
    </div>
  ),
};
