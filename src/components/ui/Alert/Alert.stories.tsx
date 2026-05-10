import type { Meta, StoryObj } from "storybook-solidjs-vite";
import { Button } from "@/components/ui/Button";
import { Alert } from "./Alert";
import "../design-system.stories.css";

const meta = {
  title: "Design System/Components/Alert",
  component: Alert,
  tags: ["autodocs"],
} satisfies Meta<typeof Alert>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Variants: Story = {
  render: () => (
    <div class="dsColumn">
      <Alert variant="info" title="Device ready">
        A physical device is connected and ready.
      </Alert>
      <Alert
        variant="warning"
        title="SDK path missing"
        action={
          <Button variant="outline" size="xs">
            Open Settings
          </Button>
        }
      >
        Configure Android SDK before running builds.
      </Alert>
      <Alert variant="error" title="Build failed">
        Gradle returned a non-zero exit code.
      </Alert>
      <Alert variant="success" title="Installed">
        The app was installed and launched successfully.
      </Alert>
    </div>
  ),
};
