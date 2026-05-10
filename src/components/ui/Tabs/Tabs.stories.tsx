import type { Meta, StoryObj } from "storybook-solidjs-vite";
import { createSignal } from "solid-js";
import { Tabs } from "./Tabs";
import "../design-system.stories.css";

const meta = {
  title: "Design System/Components/Tabs",
  component: Tabs,
  tags: ["autodocs"],
} satisfies Meta<typeof Tabs>;

export default meta;
type Story = StoryObj<typeof meta>;

export const MainSurface: Story = {
  render: () => {
    const [activeTab, setActiveTab] = createSignal("logs");

    return (
      <Tabs
        activeTab={activeTab()}
        onChange={setActiveTab}
        tabs={[
          { id: "logs", label: "Logs", badge: 42 },
          { id: "problems", label: "Problems", badge: 3 },
          { id: "settings", label: "Settings" },
        ]}
      />
    );
  },
};
