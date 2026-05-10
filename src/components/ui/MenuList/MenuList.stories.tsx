import type { Meta, StoryObj } from "storybook-solidjs-vite";
import { Badge } from "@/components/ui/Badge";
import { MenuEmptyState, MenuList, MenuListItem, MenuSectionHeader } from "./MenuList";
import "../design-system.stories.css";

const meta = {
  title: "Design System/Components/MenuList",
  component: MenuList,
  tags: ["autodocs"],
  parameters: {
    docs: {
      description: {
        component:
          "Use MenuList primitives for custom popover menus that need headers, metadata, rename/edit rows, or empty states.",
      },
    },
  },
} satisfies Meta<typeof MenuList>;

export default meta;
type Story = StoryObj<typeof meta>;

export const SavedFilters: Story = {
  render: () => (
    <div class="dsCard" style={{ width: "360px" }}>
      <MenuList role="menu">
        <MenuSectionHeader label="Saved filters" end={<Badge size="xs">3</Badge>} />
        <MenuListItem active onClick={() => {}}>
          level:error package:mine
        </MenuListItem>
        <MenuListItem mono gap="xs" onClick={() => {}}>
          tag:OkHttp
        </MenuListItem>
        <MenuListItem destructive onClick={() => {}}>
          Delete saved filter
        </MenuListItem>
      </MenuList>
    </div>
  ),
};

export const Empty: Story = {
  render: () => (
    <div class="dsCard" style={{ width: "320px" }}>
      <MenuList>
        <MenuSectionHeader label="Saved filters" />
        <MenuEmptyState>No filters match this search.</MenuEmptyState>
      </MenuList>
    </div>
  ),
};
