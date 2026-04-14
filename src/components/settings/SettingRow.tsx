import { type JSX, Show } from "solid-js";
import { Toggle } from "@/components/ui/Toggle";
import { Select } from "@/components/ui/Select";
import { TagInput } from "@/components/ui/TagInput";
import { Input } from "@/components/ui/Input";

interface SettingRowProps {
  label: string;
  description?: string;
  children: JSX.Element;
}

export function SettingRow(props: SettingRowProps): JSX.Element {
  return (
    <div
      style={{
        display: "flex",
        "align-items": "flex-start",
        "justify-content": "space-between",
        gap: "16px",
        padding: "10px 0",
        "border-bottom": "1px solid var(--border)",
      }}
    >
      <div style={{ flex: "1", "min-width": "0" }}>
        <div style={{ "font-size": "13px", color: "var(--text-primary)", "font-weight": "500" }}>
          {props.label}
        </div>
        <Show when={props.description}>
          <div style={{ "font-size": "11px", color: "var(--text-muted)", "margin-top": "2px" }}>
            {props.description}
          </div>
        </Show>
      </div>
      <div style={{ "flex-shrink": "0", display: "flex", "align-items": "center" }}>
        {props.children}
      </div>
    </div>
  );
}

// ── Control components — thin wrappers over ui/ primitives ───────────────────

export function SettingToggle(props: {
  checked: boolean;
  onChange: (val: boolean) => void;
}): JSX.Element {
  return <Toggle checked={props.checked} onChange={props.onChange} />;
}

export function SettingNumberInput(props: {
  value: number;
  min?: number;
  max?: number;
  step?: number;
  onChange: (val: number) => void;
}): JSX.Element {
  return (
    <div style={{ width: "80px" }}>
      <Input
        type="number"
        value={props.value}
        onInput={(v) => {
          const n = parseInt(v, 10);
          if (!isNaN(n)) props.onChange(n);
        }}
      />
    </div>
  );
}

export function SettingTextInput(props: {
  value: string;
  placeholder?: string;
  onChange: (val: string) => void;
}): JSX.Element {
  return (
    <div style={{ width: "200px" }}>
      <Input
        type="text"
        value={props.value}
        placeholder={props.placeholder}
        onInput={props.onChange}
      />
    </div>
  );
}

export function SettingSelect(props: {
  value: string;
  options: string[];
  onChange: (val: string) => void;
}): JSX.Element {
  return <Select value={props.value} options={props.options} onChange={props.onChange} />;
}

export function SettingTagList(props: {
  tags: string[];
  onChange: (tags: string[]) => void;
}): JSX.Element {
  return <TagInput tags={props.tags} onChange={props.onChange} />;
}

export default SettingRow;
