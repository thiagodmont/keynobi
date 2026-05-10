import { createSignal } from "solid-js";
import type { Meta, StoryObj } from "storybook-solidjs-vite";
import {
  Button,
  ControlStrip,
  FilterChip,
  Icon,
  IconButton,
  Toolbar,
  Toggle,
} from "@/components/ui";
import "./design-system.stories.css";

const meta = {
  title: "Design System/Actions",
  tags: ["autodocs"],
  parameters: {
    docs: {
      description: {
        component:
          "Action primitives for command surfaces, toolbars, compact filters, and binary settings.",
      },
    },
  },
} satisfies Meta;

export default meta;
type Story = StoryObj<typeof meta>;

export const Buttons: Story = {
  render: () => (
    <div class="dsPage">
      <section class="dsSection">
        <h2 class="dsSectionTitle">Button Variants</h2>
        <div class="dsStack">
          <Button variant="primary">Primary</Button>
          <Button variant="secondary">Secondary</Button>
          <Button variant="outline">Outline</Button>
          <Button variant="ghost">Ghost</Button>
          <Button variant="danger">Danger</Button>
          <Button loading>Loading</Button>
          <Button disabled>Disabled</Button>
        </div>
      </section>

      <section class="dsSection">
        <h2 class="dsSectionTitle">Compact Actions</h2>
        <div class="dsStack">
          <Button variant="outline" size="xs">
            Save filter
          </Button>
          <Button variant="outline" size="sm" tone="success">
            Run
          </Button>
          <Button variant="outline" size="sm" tone="warning">
            Pause
          </Button>
          <Button variant="outline" size="sm" tone="danger">
            Clear
          </Button>
          <IconButton title="Refresh" onClick={() => {}} size="sm">
            <Icon name="refresh" size={14} />
          </IconButton>
          <IconButton title="Delete" onClick={() => {}} active>
            <Icon name="trash" size={15} />
          </IconButton>
        </div>
      </section>
    </div>
  ),
};

export const ToolbarsAndFilters: Story = {
  render: () => {
    const [errorsOnly, setErrorsOnly] = createSignal(true);
    const [mineOnly, setMineOnly] = createSignal(false);
    const [enabled, setEnabled] = createSignal(true);

    return (
      <div class="dsPage">
        <section class="dsSection">
          <h2 class="dsSectionTitle">Control Strip</h2>
          <ControlStrip wrap>
            <Button variant="outline" size="xs">
              Start
            </Button>
            <Button variant="outline" size="xs">
              Pause
            </Button>
            <FilterChip
              active={errorsOnly()}
              activeStyle="soft"
              onClick={() => setErrorsOnly(!errorsOnly())}
            >
              level:error
            </FilterChip>
            <FilterChip active={mineOnly()} tone="muted" onClick={() => setMineOnly(!mineOnly())}>
              package:mine
            </FilterChip>
            <Toggle checked={enabled()} size="sm" onChange={setEnabled} />
          </ControlStrip>
        </section>

        <section class="dsSection">
          <h2 class="dsSectionTitle">Toolbar</h2>
          <Toolbar
            compact
            items={[
              { id: "run", label: "Run", onClick: () => {} },
              { id: "stop", label: "Stop", onClick: () => {}, disabled: true },
              { id: "sep", label: "Divider", onClick: () => {}, separator: true },
              { id: "logs", label: "Logs", onClick: () => {}, active: true },
            ]}
          />
        </section>
      </div>
    );
  },
};

export const DenseLogcatControls: Story = {
  render: () => {
    const [paused, setPaused] = createSignal(false);
    const [lifecycle, setLifecycle] = createSignal(true);

    return (
      <div class="dsPage">
        <section class="dsSection">
          <h2 class="dsSectionTitle">Dense Logcat Controls</h2>
          <ControlStrip wrap>
            <Button variant="outline" size="xs" tone="success">
              <Icon name="play" size={13} /> Start
            </Button>
            <Button
              variant="outline"
              size="xs"
              tone={paused() ? "warning" : "muted"}
              ariaPressed={paused()}
              onClick={() => setPaused(!paused())}
            >
              <Icon name={paused() ? "play" : "pause"} size={12} />
              {paused() ? "Resume" : "Pause"}
            </Button>
            <Button variant="outline" size="xs" tone="muted">
              <Icon name="refresh" size={12} /> Restart
            </Button>
            <Button variant="outline" size="xs" tone="danger">
              <Icon name="bolt" size={12} /> 3
            </Button>
            <Button variant="outline" size="xs" tone="accent">
              <Icon name="copy" size={12} /> 2 rows
            </Button>
            <FilterChip
              active={lifecycle()}
              ariaPressed={lifecycle()}
              onClick={() => setLifecycle(!lifecycle())}
            >
              Lifecycle
            </FilterChip>
            <Button variant="outline" size="xs" tone="muted">
              <Icon name="download" size={12} /> Export
            </Button>
          </ControlStrip>
        </section>
      </div>
    );
  },
};
