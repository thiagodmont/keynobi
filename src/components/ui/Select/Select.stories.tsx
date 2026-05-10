import type { Meta, StoryObj } from "storybook-solidjs-vite";
import { createSignal } from "solid-js";
import { Select } from "./Select";
import "../design-system.stories.css";

const meta = {
  title: "Design System/Components/Select",
  component: Select,
  tags: ["autodocs"],
} satisfies Meta<typeof Select>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Basic: Story = {
  render: () => {
    const [value, setValue] = createSignal("debug");

    return (
      <div style={{ width: "260px" }}>
        <Select
          value={value()}
          onChange={setValue}
          options={[
            { label: "Debug", value: "debug" },
            { label: "Release", value: "release" },
            { label: "Staging", value: "staging" },
          ]}
        />
      </div>
    );
  },
};
