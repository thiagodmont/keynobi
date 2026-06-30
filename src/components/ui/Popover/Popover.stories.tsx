import type { Meta, StoryObj } from "storybook-solidjs-vite";
import { createSignal } from "solid-js";
import { Button, Input, MenuList, MenuListItem, MenuSectionHeader } from "@/components/ui";
import { Popover } from "./Popover";
import "../design-system.stories.css";

const meta = {
  title: "Design System/Components/Popover",
  component: Popover,
  tags: ["autodocs"],
} satisfies Meta<typeof Popover>;

export default meta;
type Story = StoryObj;

export const Controlled: Story = {
  render: () => {
    const [open, setOpen] = createSignal(true);

    return (
      <Popover
        open={open()}
        onOpenChange={setOpen}
        minWidth="280px"
        trigger={(api) => (
          <Button variant="outline" size="sm" onClick={api.toggle}>
            Variables
          </Button>
        )}
      >
        <div class="dsColumn">
          <Input size="sm" mono value="${package}" ariaLabel="Variable template" />
          <MenuList role="menu">
            <MenuSectionHeader label="Insert variable" />
            <MenuListItem onClick={() => {}}>package</MenuListItem>
            <MenuListItem onClick={() => {}}>action_name</MenuListItem>
          </MenuList>
        </div>
      </Popover>
    );
  },
};
