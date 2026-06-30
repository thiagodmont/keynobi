import type { Meta, StoryObj } from "storybook-solidjs-vite";
import { Button } from "@/components/ui/Button";
import { Dropdown } from "./Dropdown";
import "../design-system.stories.css";

const meta = {
  title: "Design System/Components/Dropdown",
  component: Dropdown,
  tags: ["autodocs"],
} satisfies Meta<typeof Dropdown>;

export default meta;
type Story = StoryObj;

export const Menu: Story = {
  render: () => (
    <Dropdown
      trigger={
        <Button variant="outline" size="sm">
          Package
        </Button>
      }
      items={[
        { label: "All packages", onClick: () => {} },
        { label: "Mine only", onClick: () => {} },
        { separator: true },
        { label: "Clear selection", destructive: true, onClick: () => {} },
      ]}
    />
  ),
};
