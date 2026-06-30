import { onCleanup, onMount } from "solid-js";
import type { Meta, StoryObj } from "storybook-solidjs-vite";
import { registerAction, clearActions } from "@/lib/action-registry";
import { Button } from "@/components/ui/Button";
import { CommandPalette, openPalette } from "./CommandPalette";
import "../design-system.stories.css";

const meta = {
  title: "Design System/Components/CommandPalette",
  component: CommandPalette,
  tags: ["autodocs"],
  parameters: {
    docs: {
      description: {
        component:
          "CommandPalette displays actions registered through the action registry. App commands should use the registry so they are discoverable.",
      },
    },
  },
} satisfies Meta<typeof CommandPalette>;

export default meta;
type Story = StoryObj;

function registerStoryActions(): void {
  clearActions();
  registerAction({
    id: "view.logcat",
    label: "Open Logcat Panel",
    category: "View",
    shortcut: "Cmd+2",
    icon: "terminal",
    action: () => {},
  });
  registerAction({
    id: "build.run",
    label: "Run App",
    category: "Build",
    shortcut: "Cmd+R",
    icon: "play",
    action: () => {},
  });
  registerAction({
    id: "general.settings",
    label: "Open Settings",
    category: "General",
    shortcut: "Cmd+,",
    icon: "gear",
    action: () => {},
  });
}

export const Open: Story = {
  render: () => {
    onMount(() => {
      registerStoryActions();
      openPalette("commands");
    });
    onCleanup(() => clearActions());

    return (
      <>
        <Button variant="outline" onClick={() => openPalette("commands")}>
          Open command palette
        </Button>
        <CommandPalette />
      </>
    );
  },
};
