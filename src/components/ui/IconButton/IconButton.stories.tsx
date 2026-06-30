import type { Meta, StoryObj } from "storybook-solidjs-vite";
import { Icon } from "@/components/ui/Icon";
import { IconButton } from "./IconButton";
import "../design-system.stories.css";

const meta = {
  title: "Design System/Components/IconButton",
  component: IconButton,
  tags: ["autodocs"],
  parameters: {
    docs: {
      description: {
        component:
          "Use IconButton for compact toolbar commands. Always provide a meaningful title for accessible naming.",
      },
    },
  },
} satisfies Meta<typeof IconButton>;

export default meta;
type Story = StoryObj;

export const States: Story = {
  render: () => (
    <div class="dsStack">
      <IconButton title="Refresh" onClick={() => {}}>
        <Icon name="refresh" size={15} />
      </IconButton>
      <IconButton title="Pinned" active onClick={() => {}}>
        <Icon name="pin" size={15} />
      </IconButton>
      <IconButton title="Delete" disabled onClick={() => {}}>
        <Icon name="trash" size={15} />
      </IconButton>
      <IconButton title="Small refresh" size="sm" onClick={() => {}}>
        <Icon name="refresh" size={13} />
      </IconButton>
    </div>
  ),
};
