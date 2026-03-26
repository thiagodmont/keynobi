import { type JSX, Show, For } from "solid-js";

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

// ── Control components ───────────────────────────────────────────────────────

const inputStyle = {
  background: "var(--bg-primary)",
  border: "1px solid var(--border)",
  color: "var(--text-primary)",
  padding: "4px 8px",
  "border-radius": "4px",
  "font-size": "12px",
  "font-family": "inherit",
  outline: "none",
  width: "80px",
};

export function SettingToggle(props: {
  checked: boolean;
  onChange: (val: boolean) => void;
}): JSX.Element {
  return (
    <button
      role="switch"
      aria-checked={props.checked}
      onClick={() => props.onChange(!props.checked)}
      style={{
        width: "36px",
        height: "20px",
        "border-radius": "10px",
        border: "none",
        cursor: "pointer",
        position: "relative",
        background: props.checked ? "var(--accent)" : "var(--bg-quaternary)",
        transition: "background 0.15s",
      }}
    >
      <span
        style={{
          position: "absolute",
          top: "2px",
          left: props.checked ? "18px" : "2px",
          width: "16px",
          height: "16px",
          "border-radius": "50%",
          background: "#fff",
          transition: "left 0.15s",
        }}
      />
    </button>
  );
}

export function SettingNumberInput(props: {
  value: number;
  min?: number;
  max?: number;
  step?: number;
  onChange: (val: number) => void;
}): JSX.Element {
  return (
    <input
      type="number"
      value={props.value}
      min={props.min}
      max={props.max}
      step={props.step ?? 1}
      onInput={(e) => {
        const v = parseInt(e.currentTarget.value, 10);
        if (!isNaN(v)) props.onChange(v);
      }}
      style={inputStyle}
    />
  );
}

export function SettingTextInput(props: {
  value: string;
  placeholder?: string;
  onChange: (val: string) => void;
}): JSX.Element {
  return (
    <input
      type="text"
      value={props.value}
      placeholder={props.placeholder}
      onInput={(e) => props.onChange(e.currentTarget.value)}
      style={{ ...inputStyle, width: "200px" }}
    />
  );
}

export function SettingSelect(props: {
  value: string;
  options: string[];
  onChange: (val: string) => void;
}): JSX.Element {
  return (
    <select
      value={props.value}
      onChange={(e) => props.onChange(e.currentTarget.value)}
      style={{
        ...inputStyle,
        width: "120px",
        cursor: "pointer",
      }}
    >
      <For each={props.options}>
        {(opt) => <option value={opt}>{opt}</option>}
      </For>
    </select>
  );
}

export function SettingTagList(props: {
  tags: string[];
  onChange: (tags: string[]) => void;
}): JSX.Element {
  let inputRef!: HTMLInputElement;

  function addTag() {
    const val = inputRef.value.trim();
    if (val && !props.tags.includes(val)) {
      props.onChange([...props.tags, val]);
      inputRef.value = "";
    }
  }

  function removeTag(tag: string) {
    props.onChange(props.tags.filter((t) => t !== tag));
  }

  return (
    <div style={{ display: "flex", "flex-direction": "column", gap: "4px", "max-width": "250px" }}>
      <div style={{ display: "flex", "flex-wrap": "wrap", gap: "4px" }}>
        <For each={props.tags}>
          {(tag) => (
            <span
              style={{
                display: "inline-flex",
                "align-items": "center",
                gap: "4px",
                padding: "2px 6px",
                background: "var(--bg-quaternary)",
                "border-radius": "3px",
                "font-size": "11px",
                color: "var(--text-secondary)",
              }}
            >
              {tag}
              <button
                onClick={() => removeTag(tag)}
                style={{
                  background: "none",
                  border: "none",
                  color: "var(--text-muted)",
                  cursor: "pointer",
                  padding: "0",
                  "font-size": "12px",
                  "line-height": "1",
                }}
              >
                x
              </button>
            </span>
          )}
        </For>
      </div>
      <div style={{ display: "flex", gap: "4px" }}>
        <input
          ref={inputRef}
          type="text"
          placeholder="Add..."
          onKeyDown={(e) => { if (e.key === "Enter") addTag(); }}
          style={{ ...inputStyle, width: "120px", "font-size": "11px" }}
        />
        <button
          onClick={addTag}
          style={{
            background: "var(--bg-quaternary)",
            border: "1px solid var(--border)",
            color: "var(--text-secondary)",
            padding: "2px 8px",
            "border-radius": "4px",
            cursor: "pointer",
            "font-size": "11px",
          }}
        >
          Add
        </button>
      </div>
    </div>
  );
}

export default SettingRow;
