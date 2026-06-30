import type { Meta, StoryObj } from "storybook-solidjs-vite";
import { CopyableText } from "./CopyableText";
import "../design-system.stories.css";

const meta = {
  title: "Design System/Components/CopyableText",
  component: CopyableText,
  tags: ["autodocs"],
} satisfies Meta<typeof CopyableText>;

export default meta;
type Story = StoryObj;

export const States: Story = {
  render: () => (
    <div class="dsColumn" style={{ width: "420px" }}>
      <CopyableText text="com.example.app" mono />
      <CopyableText text="A long log message that can be copied from a dense panel" truncate />
      <CopyableText text="Icon only copy target" iconOnly />
    </div>
  ),
};
