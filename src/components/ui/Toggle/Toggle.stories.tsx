import type { Meta, StoryObj } from "storybook-solidjs-vite";
import { createSignal } from "solid-js";
import { Toggle } from "./Toggle";
import "../design-system.stories.css";

const meta = {
  title: "Design System/Components/Toggle",
  component: Toggle,
  tags: ["autodocs"],
} satisfies Meta<typeof Toggle>;

export default meta;
type Story = StoryObj;

export const States: Story = {
  render: () => {
    const [checked, setChecked] = createSignal(true);

    return (
      <div class="dsStack">
        <Toggle checked={checked()} onChange={setChecked} />
        <Toggle checked={checked()} onChange={setChecked} size="sm" />
        <Toggle checked={false} disabled onChange={() => {}} />
      </div>
    );
  },
};
