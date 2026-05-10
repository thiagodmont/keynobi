import { createSignal } from "solid-js";
import type { Meta, StoryObj } from "storybook-solidjs-vite";
import { Checkbox, FormField, Input, Select, TagInput, Textarea } from "@/components/ui";
import "./design-system.stories.css";

const meta = {
  title: "Design System/Forms",
  tags: ["autodocs"],
  parameters: {
    docs: {
      description: {
        component:
          "Form controls for settings, filters, inline editing, and dense developer workflows.",
      },
    },
  },
} satisfies Meta;

export default meta;
type Story = StoryObj<typeof meta>;

export const Controls: Story = {
  render: () => {
    const [query, setQuery] = createSignal("level:error package:mine");
    const [variant, setVariant] = createSignal("debug");
    const [checked, setChecked] = createSignal(true);
    const [tags, setTags] = createSignal(["ActivityManager", "OkHttp"]);

    return (
      <div class="dsPage">
        <div class="dsGrid">
          <div class="dsCard">
            <div class="dsCardTitle">Search and Text Inputs</div>
            <Input
              type="search"
              size="sm"
              value={query()}
              mono
              clearable
              placeholder="Filter logs"
              prefix="Query"
              onInput={setQuery}
              onClear={() => setQuery("")}
            />
            <Input size="xs" value="com.example.app" mono />
            <Input size="sm" state="error" value="missing:value" mono />
          </div>

          <div class="dsCard">
            <div class="dsCardTitle">Field Composition</div>
            <FormField
              id="variant"
              label="Build variant"
              description="Use Select for compact option lists."
              required
            >
              <Select
                value={variant()}
                onChange={setVariant}
                options={[
                  { label: "Debug", value: "debug" },
                  { label: "Release", value: "release" },
                ]}
              />
            </FormField>
            <Checkbox checked={checked()} onChange={setChecked}>
              Enable lifecycle logs
            </Checkbox>
          </div>
        </div>

        <div class="dsGrid">
          <div class="dsCard">
            <div class="dsCardTitle">Tag Input</div>
            <TagInput tags={tags()} onChange={setTags} max={5} placeholder="Add tag" />
          </div>

          <div class="dsCard">
            <div class="dsCardTitle">Textarea</div>
            <Textarea
              rows={5}
              resize="vertical"
              mono
              value={"Fatal Exception: main\njava.lang.IllegalStateException"}
            />
          </div>
        </div>
      </div>
    );
  },
};
