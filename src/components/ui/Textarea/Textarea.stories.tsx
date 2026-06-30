import type { Meta, StoryObj } from "storybook-solidjs-vite";
import { Textarea } from "./Textarea";
import "../design-system.stories.css";

const meta = {
  title: "Design System/Components/Textarea",
  component: Textarea,
  tags: ["autodocs"],
} satisfies Meta<typeof Textarea>;

export default meta;
type Story = StoryObj;

export const States: Story = {
  render: () => (
    <div class="dsColumn" style={{ width: "420px" }}>
      <Textarea rows={5} resize="vertical" value="Fatal Exception: main" mono />
      <Textarea rows={3} state="error" value="Invalid Gradle output pattern" />
      <Textarea rows={3} state="disabled" value="Read-only generated text" />
    </div>
  ),
};
