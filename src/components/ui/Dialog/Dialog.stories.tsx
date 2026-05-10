import type { Meta, StoryObj } from "storybook-solidjs-vite";
import { Button } from "@/components/ui/Button";
import { DialogHost, showDialog } from "./Dialog";
import "../design-system.stories.css";

const meta = {
  title: "Design System/Components/Dialog",
  component: DialogHost,
  tags: ["autodocs"],
} satisfies Meta<typeof DialogHost>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Host: Story = {
  render: () => (
    <>
      <Button
        variant="outline"
        onClick={() => {
          void showDialog({
            title: "Clear logs?",
            message: "This clears displayed entries and the in-memory buffer.",
            buttons: [
              { label: "Cancel", value: "cancel", style: "secondary" },
              { label: "Clear", value: "clear", style: "danger" },
            ],
          });
        }}
      >
        Open dialog
      </Button>
      <DialogHost />
    </>
  ),
};
