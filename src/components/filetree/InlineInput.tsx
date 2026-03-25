import { onMount, type JSX } from "solid-js";

const INPUT_STYLE = {
  width: "100%",
  background: "var(--bg-primary)",
  color: "var(--text-primary)",
  border: "1px solid var(--accent)",
  "border-radius": "2px",
  "font-size": "13px",
  "font-family": "var(--font-ui)",
  padding: "1px 4px",
  outline: "none",
  height: "20px",
} as const;

interface InlineInputProps {
  /** Initial value to populate the input (e.g. current filename for rename). */
  initialValue?: string;
  /** Called with the trimmed value when the user presses Enter. */
  onConfirm: (value: string) => void;
  /** Called when the user presses Escape or the input blurs without confirming. */
  onCancel: () => void;
  /** Indentation depth in pixels (matches tree row padding). */
  indent: number;
}

/**
 * A single-line text input rendered inline within the file tree.
 * Used for "New File", "New Folder", and "Rename" operations.
 *
 * Behaviour:
 *  - Auto-focuses and selects the filename (excluding the extension for renames)
 *  - Enter confirms, Escape cancels
 *  - Blur cancels (clicking elsewhere dismisses)
 */
export function InlineInput(props: InlineInputProps): JSX.Element {
  let inputRef!: HTMLInputElement;

  onMount(() => {
    inputRef.focus();
    const value = props.initialValue ?? "";
    if (value) {
      // Select just the name part (before the last dot), matching VS Code behaviour.
      const dotIdx = value.lastIndexOf(".");
      if (dotIdx > 0) {
        inputRef.setSelectionRange(0, dotIdx);
      } else {
        inputRef.select();
      }
    }
  });

  function handleKeyDown(e: KeyboardEvent) {
    if (e.key === "Enter") {
      e.preventDefault();
      const value = inputRef.value.trim();
      if (value) {
        props.onConfirm(value);
      } else {
        props.onCancel();
      }
    } else if (e.key === "Escape") {
      e.preventDefault();
      props.onCancel();
    }
  }

  function handleBlur() {
    props.onCancel();
  }

  return (
    <div
      style={{
        display: "flex",
        "align-items": "center",
        padding: `2px 8px 2px ${props.indent + 8}px`,
        height: "22px",
        gap: "4px",
      }}
    >
      {/* Spacer for chevron + icon alignment */}
      <span style={{ width: "28px", "flex-shrink": "0" }} />
      <input
        ref={inputRef}
        type="text"
        value={props.initialValue ?? ""}
        onKeyDown={handleKeyDown}
        onBlur={handleBlur}
        style={INPUT_STYLE}
      />
    </div>
  );
}
