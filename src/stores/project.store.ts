import { createStore } from "solid-js/store";

interface ProjectState {
  projectRoot: string | null;
  /** Detected Gradle project root (ancestor with settings.gradle). */
  gradleRoot: string | null;
  projectName: string | null;
  loading: boolean;
}

const [projectState, setProjectState] = createStore<ProjectState>({
  projectRoot: null,
  gradleRoot: null,
  projectName: null,
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

export function clearProject() {
  setProjectState({
    projectRoot: null,
    gradleRoot: null,
    projectName: null,
    loading: false,
  });
}

export function setLoading(loading: boolean) {
  setProjectState("loading", loading);
}
