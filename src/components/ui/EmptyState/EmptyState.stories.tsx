import type { Meta, StoryObj } from "storybook-solidjs-vite";
import { Button } from "@/components/ui/Button";
import { EmptyState } from "./EmptyState";
import "../design-system.stories.css";

const meta = {
  title: "Design System/Components/EmptyState",
  component: EmptyState,
  tags: ["autodocs"],
} satisfies Meta<typeof EmptyState>;

export default meta;
type Story = StoryObj;

export const Densities: Story = {
  render: () => (
    <div class="dsGrid">
      <EmptyState
        icon="terminal"
        title="No logs yet"
        description="Start Logcat to stream entries from the selected device."
        action={
          <Button variant="outline" size="sm">
            Start
          </Button>
        }
      />
      <EmptyState
        icon="search"
        title="No matches"
        description="Try a broader query."
        density="compact"
      />
    </div>
  ),
};
