import { For } from "solid-js";
import type { Meta, StoryObj } from "storybook-solidjs-vite";
import { Icon } from "./Icon";
import "../design-system.stories.css";

const meta = {
  title: "Design System/Components/Icon",
  component: Icon,
  tags: ["autodocs"],
} satisfies Meta<typeof Icon>;

export default meta;
type Story = StoryObj;

const icons = [
  "folder",
  "terminal",
  "play",
  "pause",
  "stop",
  "refresh",
  "arrow-up",
  "arrow-down",
  "search",
  "gear",
  "warning",
  "error-circle",
  "bolt",
  "check",
  "copy",
  "trash",
  "external-link",
  "device",
];

export const Library: Story = {
  render: () => (
    <div class="dsGrid">
      <For each={icons}>
        {(name) => (
          <div class="dsStack">
            <Icon name={name} size={18} />
            <span class="dsCode">{name}</span>
          </div>
        )}
      </For>
    </div>
  ),
};
