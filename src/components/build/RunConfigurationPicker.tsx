import { type JSX, createMemo } from "solid-js";
import { Select, type SelectOption, showToast } from "@/components/ui";
import { formatError } from "@/lib/tauri-api";
import { activeRunConfiguration, runConfigState } from "@/stores/run-configurations.store";
import {
  chooseRunConfiguration,
  openRunConfigurationsEditor,
} from "@/services/run-configurations.service";
import styles from "./RunConfigurationPicker.module.css";

/** The option that opens the editor instead of choosing a configuration. */
export const EDIT_RUN_CONFIGURATIONS = "__edit__";

/** Title-bar picker of the run configuration Run App and Build Only use. */
export function RunConfigurationPicker(props: { disabled?: boolean }): JSX.Element {
  let root!: HTMLDivElement;

  const options = createMemo<SelectOption[]>(() => [
    ...runConfigState.configurations.map((c) => ({ label: c.name, value: c.name })),
    { label: "Edit Configurations…", value: EDIT_RUN_CONFIGURATIONS },
  ]);

  const title = () => {
    const active = activeRunConfiguration();
    if (!active) return "Choose the run configuration Run App uses";
    return `Run configuration: ${active.name} (${active.module} · ${active.variant})`;
  };

  function handleChange(value: string) {
    if (value === EDIT_RUN_CONFIGURATIONS) {
      // Keep showing the active configuration; this option only opens the editor.
      const select = root.querySelector("select");
      if (select) select.value = runConfigState.active ?? "";
      openRunConfigurationsEditor();
      return;
    }
    chooseRunConfiguration(value).catch((e) => {
      showToast(`Failed to choose the run configuration: ${formatError(e)}`, "error");
    });
  }

  return (
    <div
      ref={root}
      class={styles.root}
      data-testid="run-configuration-picker"
      onMouseDown={(e) => e.stopPropagation()}
    >
      <Select
        id="run-configuration-picker"
        class={styles.select}
        ariaLabel="Run configuration"
        title={title()}
        value={runConfigState.active ?? ""}
        placeholder={
          runConfigState.configurations.length ? "Choose a configuration" : "No configurations"
        }
        options={options()}
        onChange={handleChange}
        disabled={props.disabled}
      />
    </div>
  );
}

/** Move keyboard focus to the title-bar picker ("Select Run Configuration…"). */
export function focusRunConfigurationPicker(): void {
  const select = document.getElementById("run-configuration-picker") as HTMLSelectElement | null;
  if (!select) return;
  select.focus();
  try {
    select.showPicker?.();
  } catch {
    // Not supported everywhere; focus is enough to choose with the keyboard.
  }
}
