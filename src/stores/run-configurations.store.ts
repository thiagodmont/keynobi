import { createStore } from "solid-js/store";
import type {
  LocalRunState,
  ProjectRunConfigurations,
  RunConfiguration,
  SharedRunConfigurationsFile,
} from "@/bindings";

export interface RunConfigurationsState {
  /** The project they belong to; null when none is loaded. */
  projectRoot: string | null;
  configurations: RunConfiguration[];
  active: string | null;
  local: Record<string, LocalRunState>;
  /** Names of the configurations shared with the project. */
  shared: string[];
  /** The project's shared file; null when it has none. */
  sharedFile: SharedRunConfigurationsFile | null;
  /** The Edit Run Configurations dialog is open. */
  editorOpen: boolean;
}

const initialState = (): RunConfigurationsState => ({
  projectRoot: null,
  configurations: [],
  active: null,
  local: {},
  shared: [],
  sharedFile: null,
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
    shared: project.shared,
    sharedFile: project.sharedFile,
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

/** Whether the configuration named `name` is shared with the project. */
export function isSharedRunConfiguration(name: string): boolean {
  return runConfigState.shared.includes(name);
}

/** Test helper. */
export function resetRunConfigurationsForTests(): void {
  setRunConfigState(initialState());
}
