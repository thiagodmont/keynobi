import { createStore } from "solid-js/store";
import type { LocalRunState, ProjectRunConfigurations, RunConfiguration } from "@/bindings";

export interface RunConfigurationsState {
  /** The project they belong to; null when none is loaded. */
  projectRoot: string | null;
  configurations: RunConfiguration[];
  active: string | null;
  local: Record<string, LocalRunState>;
  /** The Edit Run Configurations dialog is open. */
  editorOpen: boolean;
}

const initialState = (): RunConfigurationsState => ({
  projectRoot: null,
  configurations: [],
  active: null,
  local: {},
  editorOpen: false,
});

const [runConfigState, setRunConfigState] = createStore<RunConfigurationsState>(initialState());

export { runConfigState };

/** Show a project's configurations as the backend returned them. */
export function setRunConfigurations(projectRoot: string, project: ProjectRunConfigurations): void {
  setRunConfigState({
    projectRoot,
    configurations: project.configurations,
    active: project.active,
    local: project.local as Record<string, LocalRunState>,
  });
}

export function resetRunConfigurations(): void {
  setRunConfigState({ ...initialState(), editorOpen: runConfigState.editorOpen });
}

export function setRunConfigEditorOpen(open: boolean): void {
  setRunConfigState("editorOpen", open);
}

/** The active configuration, or null when none is chosen. */
export function activeRunConfiguration(): RunConfiguration | null {
  return runConfigState.configurations.find((c) => c.name === runConfigState.active) ?? null;
}

/** Test helper. */
export function resetRunConfigurationsForTests(): void {
  setRunConfigState(initialState());
}
