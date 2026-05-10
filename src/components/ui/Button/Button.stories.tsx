import type { Meta, StoryObj } from "storybook-solidjs-vite";
import { Button } from "./Button";
import "../design-system.stories.css";

const meta = {
  title: "Design System/Components/Button",
  component: Button,
  tags: ["autodocs"],
  parameters: {
    docs: {
      description: {
        component:
          "Use Button for labeled commands. Prefer outline/xs in dense toolbars and primary only for the main action in a surface.",
      },
    },
  },
} satisfies Meta<typeof Button>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Variants: Story = {
  render: () => (
    <div class="dsStack">
      <Button variant="primary">Primary</Button>
      <Button variant="secondary">Secondary</Button>
      <Button variant="outline">Outline</Button>
      <Button variant="ghost">Ghost</Button>
      <Button variant="danger">Danger</Button>
    </div>
  ),
};

export const SizesAndStates: Story = {
  render: () => (
    <div class="dsStack">
      <Button size="xs" variant="outline">
        Extra small
      </Button>
      <Button size="sm" variant="outline">
        Small
      </Button>
      <Button size="md" variant="secondary">
        Medium
      </Button>
      <Button loading>Loading</Button>
      <Button disabled>Disabled</Button>
      <Button ariaPressed={false} variant="outline">
        Toggle off
      </Button>
      <Button ariaPressed variant="outline">
        Toggle on
      </Button>
    </div>
  ),
};

export const Tones: Story = {
  render: () => (
    <div class="dsStack">
      <Button variant="outline" tone="muted">
        Muted
      </Button>
      <Button variant="outline" tone="accent">
        Accent
      </Button>
      <Button variant="outline" tone="success">
        Success
      </Button>
      <Button variant="outline" tone="warning">
        Warning
      </Button>
      <Button variant="outline" tone="danger">
        Danger
      </Button>
    </div>
  ),
};
