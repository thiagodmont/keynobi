import type { Meta, StoryObj } from "storybook-solidjs-vite";
import { createSignal } from "solid-js";
import { Checkbox } from "./Checkbox";
import "../design-system.stories.css";

const meta = {
  title: "Design System/Components/Checkbox",
  component: Checkbox,
  tags: ["autodocs"],
} satisfies Meta<typeof Checkbox>;

export default meta;
type Story = StoryObj;

export const States: Story = {
  render: () => {
    const [checked, setChecked] = createSignal(true);

    return (
      <div class="dsColumn">
        <Checkbox checked={checked()} onChange={setChecked}>
          Show lifecycle logs
        </Checkbox>
        <Checkbox checked={false} indeterminate onChange={() => {}}>
          Mixed package selection
        </Checkbox>
        <Checkbox checked={false} disabled onChange={() => {}}>
          Disabled setting
        </Checkbox>
      </div>
    );
  },
};
