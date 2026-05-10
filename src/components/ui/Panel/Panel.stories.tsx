import type { Meta, StoryObj } from "storybook-solidjs-vite";
import { Badge, Button } from "@/components/ui";
import { Panel } from "./Panel";
import "../design-system.stories.css";

const meta = {
  title: "Design System/Components/Panel",
  component: Panel,
  tags: ["autodocs"],
} satisfies Meta<typeof Panel>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Standard: Story = {
  render: () => (
    <div style={{ width: "420px" }}>
      <Panel
        title="Build Output"
        headerActions={
          <Button variant="outline" size="xs">
            Clear
          </Button>
        }
        footer={<span class="dsCode">Finished in 11.4s</span>}
      >
        <div class="dsStack">
          <Badge variant="success">Success</Badge>
          <span class="dsCode">:app:assembleDebug completed</span>
        </div>
      </Panel>
    </div>
  ),
};
