import type { Meta, StoryObj } from "storybook-solidjs-vite";
import { Button, FilterChip } from "@/components/ui";
import { ControlStrip } from "./ControlStrip";
import "../design-system.stories.css";

const meta = {
  title: "Design System/Components/ControlStrip",
  component: ControlStrip,
  tags: ["autodocs"],
} satisfies Meta<typeof ControlStrip>;

export default meta;
type Story = StoryObj;

export const ToolbarBand: Story = {
  render: () => (
    <ControlStrip wrap>
      <Button variant="outline" size="xs">
        Start
      </Button>
      <Button variant="outline" size="xs">
        Pause
      </Button>
      <FilterChip active onClick={() => {}}>
        level:error
      </FilterChip>
      <FilterChip active={false} onClick={() => {}}>
        package:mine
      </FilterChip>
    </ControlStrip>
  ),
};
