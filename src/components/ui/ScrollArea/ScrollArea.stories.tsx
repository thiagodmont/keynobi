import { For } from "solid-js";
import type { Meta, StoryObj } from "storybook-solidjs-vite";
import { CopyableText } from "@/components/ui/CopyableText";
import { ScrollArea } from "./ScrollArea";
import "../design-system.stories.css";

const meta = {
  title: "Design System/Components/ScrollArea",
  component: ScrollArea,
  tags: ["autodocs"],
} satisfies Meta<typeof ScrollArea>;

export default meta;
type Story = StoryObj;

export const Vertical: Story = {
  render: () => (
    <ScrollArea class="dsScrollDemo">
      <div class="dsColumn">
        <For each={Array.from({ length: 12 }, (_, index) => index + 1)}>
          {(row) => <CopyableText text={`Scrollable row ${row}`} mono />}
        </For>
      </div>
    </ScrollArea>
  ),
};
