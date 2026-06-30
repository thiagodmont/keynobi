import { createSignal, For } from "solid-js";
import type { Meta, StoryObj } from "storybook-solidjs-vite";
import {
  Badge,
  Button,
  CopyableText,
  DockedPanel,
  Dropdown,
  Input,
  MenuEmptyState,
  MenuList,
  MenuListItem,
  MenuSectionHeader,
  MetadataCell,
  MetadataGrid,
  Panel,
  Popover,
  ScrollArea,
  Tabs,
} from "@/components/ui";
import "./design-system.stories.css";

const meta = {
  title: "Design System/Surfaces",
  tags: ["autodocs"],
  parameters: {
    docs: {
      description: {
        component:
          "Surface primitives for panels, popovers, menus, metadata readouts, tabs, and scrollable regions.",
      },
    },
  },
} satisfies Meta;

export default meta;
type Story = StoryObj;

export const PanelsAndMetadata: Story = {
  render: () => (
    <div class="dsPage">
      <div class="dsGrid">
        <Panel
          title="Build Output"
          headerActions={
            <Button variant="outline" size="xs">
              Clear
            </Button>
          }
          footer={<span class="dsCode">Finished in 11.4s</span>}
        >
          <div class="dsColumn">
            <Badge variant="success" size="xs">
              Success
            </Badge>
            <span class="dsCode">:app:assembleDebug completed</span>
          </div>
        </Panel>

        <DockedPanel
          title="Entry Detail"
          titleTone="info"
          subtitle="Tap metadata to filter"
          maxHeight="260px"
          actions={
            <Button variant="outline" size="xs">
              Copy
            </Button>
          }
        >
          <MetadataGrid columns={2}>
            <MetadataCell label="Level" value={<Badge variant="error">E</Badge>} />
            <MetadataCell
              label="Tag"
              value="AndroidRuntime"
              valueStyle={{ "font-family": "var(--font-mono)" }}
            />
            <MetadataCell label="PID" value="1742" />
            <MetadataCell label="Package" value="com.example.app" />
          </MetadataGrid>
        </DockedPanel>
      </div>
    </div>
  ),
};

export const MenusAndOverlays: Story = {
  render: () => {
    const [open, setOpen] = createSignal(true);

    return (
      <div class="dsPage">
        <div class="dsGrid">
          <div class="dsCard">
            <div class="dsCardTitle">Menu List</div>
            <MenuList role="menu">
              <MenuSectionHeader label="Saved filters" end={<Badge size="xs">3</Badge>} />
              <MenuListItem active onClick={() => {}}>
                level:error package:mine
              </MenuListItem>
              <MenuListItem mono onClick={() => {}}>
                tag:OkHttp
              </MenuListItem>
              <MenuListItem destructive onClick={() => {}}>
                Delete saved filter
              </MenuListItem>
              <MenuEmptyState>No matching filters</MenuEmptyState>
            </MenuList>
          </div>

          <div class="dsCard">
            <div class="dsCardTitle">Dropdown</div>
            <Dropdown
              trigger={
                <Button variant="outline" size="sm">
                  Open menu
                </Button>
              }
              items={[
                { label: "All packages", onClick: () => {} },
                { label: "Mine only", onClick: () => {} },
                { separator: true },
                { label: "Clear selection", destructive: true, onClick: () => {} },
              ]}
            />
          </div>

          <div class="dsCard">
            <div class="dsCardTitle">Popover</div>
            <Popover
              open={open()}
              onOpenChange={setOpen}
              minWidth="260px"
              trigger={(api) => (
                <Button variant="outline" size="sm" onClick={api.toggle}>
                  Variables
                </Button>
              )}
            >
              <div class="dsColumn">
                <Input size="sm" mono value="${package}" />
                <Button variant="outline" size="xs">
                  Insert variable
                </Button>
              </div>
            </Popover>
          </div>
        </div>
      </div>
    );
  },
};

export const NavigationAndScrolling: Story = {
  render: () => {
    const [activeTab, setActiveTab] = createSignal("logs");

    return (
      <div class="dsPage">
        <Tabs
          activeTab={activeTab()}
          onChange={setActiveTab}
          tabs={[
            { id: "logs", label: "Logs", badge: 42 },
            { id: "problems", label: "Problems", badge: 3 },
            { id: "settings", label: "Settings" },
          ]}
        />

        <Panel title="Scrollable Content" class="dsPanelDemo">
          <ScrollArea class="dsScrollDemo">
            <div class="dsColumn">
              <For each={Array.from({ length: 12 }, (_, index) => index + 1)}>
                {(row) => (
                  <CopyableText text={`Log row ${row}: ActivityManager displayed activity`} mono />
                )}
              </For>
            </div>
          </ScrollArea>
        </Panel>
      </div>
    );
  },
};
