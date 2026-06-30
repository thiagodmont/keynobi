import type { Meta, StoryObj } from "storybook-solidjs-vite";
import { createSignal } from "solid-js";
import { Input } from "./Input";
import "../design-system.stories.css";

const meta = {
  title: "Design System/Components/Input",
  component: Input,
  tags: ["autodocs"],
  parameters: {
    docs: {
      description: {
        component:
          "Use Input for dense query, settings, and inline-edit fields. Use mono for code-like values.",
      },
    },
  },
} satisfies Meta<typeof Input>;

export default meta;
type Story = StoryObj;

export const Sizes: Story = {
  render: () => (
    <div class="dsColumn" style={{ width: "360px" }}>
      <Input size="xs" value="level:error" mono />
      <Input size="sm" value="package:com.example.app" mono />
      <Input size="md" value="Android SDK path" />
    </div>
  ),
};

export const SearchAndStates: Story = {
  render: () => {
    const [value, setValue] = createSignal("level:error package:mine");

    return (
      <div class="dsColumn" style={{ width: "420px" }}>
        <Input
          type="search"
          value={value()}
          onInput={setValue}
          onClear={() => setValue("")}
          clearable
          mono
          ariaLabel="Log query"
          prefix="Query"
          placeholder="Filter logs"
        />
        <Input state="error" value="bad:token" mono ariaLabel="Invalid query example" />
        <Input state="disabled" value="Disabled value" ariaLabel="Disabled input example" />
      </div>
    );
  },
};
