import type { Meta, StoryObj } from "storybook-solidjs-vite";
import { createSignal } from "solid-js";
import { TagInput } from "./TagInput";
import "../design-system.stories.css";

const meta = {
  title: "Design System/Components/TagInput",
  component: TagInput,
  tags: ["autodocs"],
} satisfies Meta<typeof TagInput>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Basic: Story = {
  render: () => {
    const [tags, setTags] = createSignal(["ActivityManager", "OkHttp"]);

    return (
      <div style={{ width: "420px" }}>
        <TagInput tags={tags()} onChange={setTags} max={5} placeholder="Add tag" />
      </div>
    );
  },
};
