import type { Meta, StoryObj } from "storybook-solidjs-vite";
import { Show, createSignal } from "solid-js";
import { Button } from "@/components/ui/Button";
import { MenuListItem } from "@/components/ui/MenuList";
import { ContextMenu } from "./ContextMenu";
import "../design-system.stories.css";

const meta = {
  title: "Design System/Components/ContextMenu",
  component: ContextMenu,
  tags: ["autodocs"],
  parameters: {
    docs: {
      description: {
        component:
          "Actions for one row, opened by right-click or Shift+F10. Focus moves to the first item; Up/Down move; Escape, Tab, or a click outside closes and returns focus. Render it only while open.",
      },
    },
  },
} satisfies Meta<typeof ContextMenu>;

export default meta;
type Story = StoryObj;

export const ProjectActions: Story = {
  render: () => {
    const [open, setOpen] = createSignal(true);
    return (
      <div style={{ height: "160px" }}>
        <Button variant="outline" size="xs" onClick={() => setOpen(true)}>
          MockProject
        </Button>
        <Show when={open()}>
          <ContextMenu
            x={24}
            y={40}
            width={176}
            label="Actions for MockProject"
            onClose={() => setOpen(false)}
          >
            <MenuListItem role="menuitem" onClick={() => setOpen(false)}>
              Rename…
            </MenuListItem>
            <MenuListItem role="menuitem" onClick={() => setOpen(false)}>
              Revoke Trust
            </MenuListItem>
            <MenuListItem role="menuitem" destructive onClick={() => setOpen(false)}>
              Remove from List
            </MenuListItem>
          </ContextMenu>
        </Show>
      </div>
    );
  },
};
