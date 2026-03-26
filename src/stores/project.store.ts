import { createStore } from "solid-js/store";

interface ProjectState {
  projectRoot: string | null;
  /** Detected Gradle project root (ancestor with settings.gradle). */
  gradleRoot: string | null;
  projectName: string | null;
  /** Application ID read from build.gradle(.kts). Used for `package:mine`. */
  applicationId: string | null;
  loading: boolean;
}

const [projectState, setProjectState] = createStore<ProjectState>({
  projectRoot: null,
  gradleRoot: null,
  projectName: null,
  applicationId: null,
  loading: false,
});

export { projectState, setProjectState };

export function setProject(root: string, projectNameOrTree: string | object, gradleRoot?: string | null) {
  const name = typeof projectNameOrTree === "string"
    ? projectNameOrTree
    : root.split("/").filter(Boolean).pop() ?? root;
  setProjectState({
    projectRoot: root,
    gradleRoot: gradleRoot ?? null,
    projectName: name,
    loading: false,
  });
}

export function setApplicationId(id: string | null) {
  setProjectState("applicationId", id);
}

export function clearProject() {
  setProjectState({
    projectRoot: null,
    gradleRoot: null,
    projectName: null,
    applicationId: null,
    loading: false,
  });
}

export function setLoading(loading: boolean) {
  setProjectState("loading", loading);
}
