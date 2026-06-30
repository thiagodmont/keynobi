import type { Meta, StoryObj } from "storybook-solidjs-vite";
import { createSignal } from "solid-js";
import { FilterChip } from "./FilterChip";
import "../design-system.stories.css";

const meta = {
  title: "Design System/Components/FilterChip",
  component: FilterChip,
  tags: ["autodocs"],
  parameters: {
    docs: {
      description: {
        component:
          "Use FilterChip for compact toggle filters. Keep labels short and stable so dense toolbars do not shift.",
      },
    },
  },
} satisfies Meta<typeof FilterChip>;

export default meta;
type Story = StoryObj;

export const States: Story = {
  render: () => {
    const [errorActive, setErrorActive] = createSignal(true);
    const [mineActive, setMineActive] = createSignal(false);

    return (
      <div class="dsStack">
        <FilterChip active={errorActive()} onClick={() => setErrorActive(!errorActive())}>
          level:error
        </FilterChip>
        <FilterChip
          active={mineActive()}
          activeStyle="soft"
          tone="muted"
          onClick={() => setMineActive(!mineActive())}
        >
          package:mine
        </FilterChip>
        <FilterChip active={false} maxWidth="140px" onClick={() => {}}>
          message:network timeout
        </FilterChip>
      </div>
    );
  },
};
