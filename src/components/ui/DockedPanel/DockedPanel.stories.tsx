import type { Meta, StoryObj } from "storybook-solidjs-vite";
import { Badge, Button, MetadataCell, MetadataGrid } from "@/components/ui";
import { DockedPanel } from "./DockedPanel";
import "../design-system.stories.css";

const meta = {
  title: "Design System/Components/DockedPanel",
  component: DockedPanel,
  tags: ["autodocs"],
} satisfies Meta<typeof DockedPanel>;

export default meta;
type Story = StoryObj<typeof meta>;

export const LogEntryDetail: Story = {
  render: () => (
    <div style={{ width: "560px" }}>
      <DockedPanel
        title="Entry Detail"
        subtitle="Tap metadata to filter"
        titleTone="info"
        actions={
          <Button variant="outline" size="xs">
            Copy
          </Button>
        }
      >
        <MetadataGrid columns={3}>
          <MetadataCell label="Level" value={<Badge variant="error">E</Badge>} />
          <MetadataCell label="Tag" value="AndroidRuntime" />
          <MetadataCell label="PID" value="1742" />
          <MetadataCell label="Package" value="com.example.app" />
          <MetadataCell label="Time" value="16:42:03.924" />
          <MetadataCell label="Thread" value="main" />
        </MetadataGrid>
      </DockedPanel>
    </div>
  ),
};
