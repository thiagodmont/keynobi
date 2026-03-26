import { createStore } from "solid-js/store";
import type { FileNode } from "@/bindings";

export type { FileNode };

interface ProjectState {
  projectRoot: string | null;
  /** Detected Gradle project root (ancestor with settings.gradle). */
  gradleRoot: string | null;
  projectName: string | null;
  fileTree: FileNode | null;
  loading: boolean;
}

const [projectState, setProjectState] = createStore<ProjectState>({
  projectRoot: null,
  gradleRoot: null,
  projectName: null,
  fileTree: null,
  loading: false,
});

export { projectState, setProjectState };

export function setProject(root: string, tree: FileNode, gradleRoot?: string | null) {
  const parts = root.split("/");
  const name = parts[parts.length - 1] || root;
  setProjectState({
    projectRoot: root,
    gradleRoot: gradleRoot ?? null,
    projectName: name,
    fileTree: tree,
    loading: false,
  });
}

export function clearProject() {
  setProjectState({
    projectRoot: null,
    gradleRoot: null,
    projectName: null,
    fileTree: null,
    loading: false,
  });
}

export function setLoading(loading: boolean) {
  setProjectState("loading", loading);
}
