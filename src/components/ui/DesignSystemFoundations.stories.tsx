import { For } from "solid-js";
import type { Meta, StoryObj } from "storybook-solidjs-vite";
import { Badge, Kbd, Separator } from "@/components/ui";
import "./design-system.stories.css";

const meta = {
  title: "Design System/Foundations",
  tags: ["autodocs"],
  parameters: {
    docs: {
      description: {
        component:
          "Core tokens, typography, density, and semantic states that all Keynobi UI primitives should use.",
      },
    },
  },
} satisfies Meta;

export default meta;
type Story = StoryObj;

const tokens = [
  ["--bg-primary", "var(--bg-primary)"],
  ["--bg-secondary", "var(--bg-secondary)"],
  ["--bg-tertiary", "var(--bg-tertiary)"],
  ["--accent", "var(--accent)"],
  ["--success", "var(--success)"],
  ["--warning", "var(--warning)"],
  ["--error", "var(--error)"],
  ["--border", "var(--border)"],
] as const;

export const Overview: Story = {
  render: () => (
    <div class="dsPage">
      <div class="dsIntro">
        <div class="dsEyebrow">Keynobi Design System</div>
        <h1 class="dsTitle">Foundations</h1>
        <p class="dsDescription">
          Use semantic tokens and dense, predictable layouts for app surfaces. New UI should prefer
          primitives from <span class="dsCode">@/components/ui</span> before adding local styles.
        </p>
      </div>

      <section class="dsSection">
        <h2 class="dsSectionTitle">Color Tokens</h2>
        <div class="dsSwatchGrid">
          <For each={tokens}>
            {([name, value]) => (
              <div class="dsSwatch">
                <span class="dsSwatchColor" style={{ background: value }} />
                <span class="dsSwatchName">{name}</span>
              </div>
            )}
          </For>
        </div>
      </section>

      <section class="dsSection">
        <h2 class="dsSectionTitle">Semantic Labels</h2>
        <div class="dsStack">
          <Badge variant="default">Default</Badge>
          <Badge variant="accent">Accent</Badge>
          <Badge variant="success" dot>
            Success
          </Badge>
          <Badge variant="warning" dot>
            Warning
          </Badge>
          <Badge variant="error" dot>
            Error
          </Badge>
          <Badge variant="info" size="xs">
            Info
          </Badge>
          <Badge mono size="xs">
            mono
          </Badge>
        </div>
      </section>

      <Separator />

      <section class="dsSection">
        <h2 class="dsSectionTitle">Keyboard Hints</h2>
        <div class="dsStack">
          <Kbd>Cmd</Kbd>
          <Kbd>Shift</Kbd>
          <Kbd>P</Kbd>
        </div>
      </section>
    </div>
  ),
};
