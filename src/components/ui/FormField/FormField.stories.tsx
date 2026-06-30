import type { Meta, StoryObj } from "storybook-solidjs-vite";
import { FormField } from "./FormField";
import { Input } from "@/components/ui/Input";
import "../design-system.stories.css";

const meta = {
  title: "Design System/Components/FormField",
  component: FormField,
  tags: ["autodocs"],
} satisfies Meta<typeof FormField>;

export default meta;
type Story = StoryObj;

export const States: Story = {
  render: () => (
    <div class="dsColumn" style={{ width: "360px" }}>
      <FormField
        id="sdk-path"
        label="Android SDK path"
        description="Used for ADB, emulator, and health checks."
        required
      >
        <Input value="/Users/me/Library/Android/sdk" />
      </FormField>
      <FormField id="package" label="Package" error="Package name is required.">
        <Input state="error" value="" placeholder="com.example.app" />
      </FormField>
    </div>
  ),
};
