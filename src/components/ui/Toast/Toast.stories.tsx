import type { Meta, StoryObj } from "storybook-solidjs-vite";
import { Button } from "@/components/ui/Button";
import { ToastContainer, showToast } from "./Toast";
import "../design-system.stories.css";

const meta = {
  title: "Design System/Components/Toast",
  component: ToastContainer,
  tags: ["autodocs"],
} satisfies Meta<typeof ToastContainer>;

export default meta;
type Story = StoryObj;

export const Host: Story = {
  render: () => (
    <>
      <div class="dsStack">
        <Button variant="outline" onClick={() => showToast("Build completed", "success")}>
          Success
        </Button>
        <Button variant="outline" onClick={() => showToast("ADB path missing", "warning")}>
          Warning
        </Button>
        <Button variant="outline" onClick={() => showToast("Build failed", "error")}>
          Error
        </Button>
      </div>
      <ToastContainer />
    </>
  ),
};
