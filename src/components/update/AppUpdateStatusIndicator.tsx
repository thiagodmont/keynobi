import { type JSX, Show } from "solid-js";
import { Icon, showToast } from "@/components/ui";
import { formatError } from "@/lib/tauri-api";
import { openUpdateRelease, updateState } from "@/services/update.service";

export function AppUpdateStatusIndicator(): JSX.Element {
  const update = () => updateState.update;

  return (
    <Show when={update()?.available ? update() : null}>
      {(availableUpdate) => (
        <button
          type="button"
          aria-label="Update available"
          title={`Keynobi ${availableUpdate().latestVersion} is available — click to open downloads`}
          onClick={() => {
            openUpdateRelease(availableUpdate()).catch((err) => {
              showToast(`Failed to open release page: ${formatError(err)}`, "error");
            });
          }}
          onMouseDown={(e) => e.stopPropagation()}
          style={{
            display: "flex",
            "align-items": "center",
            gap: "4px",
            padding: "0 6px",
            height: "18px",
            background: "rgba(255,255,255,0.12)",
            border: "1px solid rgba(255,255,255,0.24)",
            "border-radius": "3px",
            cursor: "pointer",
            "flex-shrink": "0",
            color: "#ffffff",
            transition: "background 0.1s",
          }}
          onMouseEnter={(e) => {
            e.currentTarget.style.background = "rgba(255,255,255,0.2)";
          }}
          onMouseLeave={(e) => {
            e.currentTarget.style.background = "rgba(255,255,255,0.12)";
          }}
        >
          <Icon name="download" size={11} />
          <span
            style={{
              "font-size": "11px",
              "line-height": "1",
              "white-space": "nowrap",
            }}
          >
            Update {availableUpdate().latestVersion}
          </span>
        </button>
      )}
    </Show>
  );
}
