import type { Meta, StoryObj } from "storybook-solidjs-vite";
import { Badge } from "@/components/ui/Badge";
import { MetadataCell, MetadataGrid } from "./MetadataGrid";
import "../design-system.stories.css";

const meta = {
  title: "Design System/Components/MetadataGrid",
  component: MetadataGrid,
  tags: ["autodocs"],
} satisfies Meta<typeof MetadataGrid>;

export default meta;
type Story = StoryObj;

export const Readout: Story = {
  render: () => (
    <div style={{ width: "520px" }}>
      <MetadataGrid columns={3}>
        <MetadataCell label="Level" value={<Badge variant="error">E</Badge>} />
        <MetadataCell label="Tag" value="AndroidRuntime" onClick={() => {}} />
        <MetadataCell label="Package" value="com.example.app" />
        <MetadataCell label="PID" value="1742" />
        <MetadataCell label="TID" value="1742" />
        <MetadataCell label="Time" value="16:42:03.924" />
      </MetadataGrid>
    </div>
  ),
};
