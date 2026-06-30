import type { Meta, StoryObj } from "storybook-solidjs-vite";
import { Badge } from "./Badge";
import "../design-system.stories.css";

const meta = {
  title: "Design System/Components/Badge",
  component: Badge,
  tags: ["autodocs"],
  parameters: {
    docs: {
      description: {
        component:
          "Use Badge for compact semantic labels, status markers, and code-like tokens. Use mono for package names, tags, and filters.",
      },
    },
  },
} satisfies Meta<typeof Badge>;

export default meta;
type Story = StoryObj;

export const Variants: Story = {
  render: () => (
    <div class="dsStack">
      <Badge>Default</Badge>
      <Badge variant="accent">Accent</Badge>
      <Badge variant="success" dot>
        Success
      </Badge>
      <Badge variant="warning" dot>
        Warning
      </Badge>
      <Badge variant="error" dot>
        Error
      </Badge>
      <Badge variant="info" dot>
        Info
      </Badge>
    </div>
  ),
};

export const DenseTokens: Story = {
  render: () => (
    <div class="dsStack">
      <Badge size="xs" mono>
        tag:OkHttp
      </Badge>
      <Badge size="xs" mono variant="accent">
        package:mine
      </Badge>
      <Badge size="xs" subtle>
        subtle
      </Badge>
      <Badge size="xs" onMouseDown={() => {}} ariaLabel="Pick JSON filter">
        JSON
      </Badge>
    </div>
  ),
};
