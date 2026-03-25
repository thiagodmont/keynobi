import { createStore } from "solid-js/store";
import type { FileNode } from "@/bindings";

export type { FileNode };

interface ProjectState {
  projectRoot: string | null;
  projectName: string | null;
  fileTree: FileNode | null;
  loading: boolean;
}

const [projectState, setProjectState] = createStore<ProjectState>({
  projectRoot: null,
  projectName: null,
  fileTree: null,
  loading: false,
});

export { projectState, setProjectState };

export function setProject(root: string, tree: FileNode) {
  const parts = root.split("/");
  const name = parts[parts.length - 1] || root;
  setProjectState({
    projectRoot: root,
    projectName: name,
    fileTree: tree,
    loading: false,
  });
}

export function clearProject() {
  setProjectState({
    projectRoot: null,
    projectName: null,
    fileTree: null,
    loading: false,
  });
}

export function setLoading(loading: boolean) {
  setProjectState("loading", loading);
}
