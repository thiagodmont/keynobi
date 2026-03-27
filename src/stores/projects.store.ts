import { createStore, produce } from "solid-js/store";
import type { ProjectEntry } from "@/bindings";

interface ProjectsState {
  /** All known projects from the registry, sorted: pinned first, then by last opened. */
  projects: ProjectEntry[];
  /** The ID of the currently active project, or null if none is open. */
  activeProjectId: string | null;
  /** True while the registry is being loaded on startup. */
  loading: boolean;
}

const [projectsState, setProjectsState] = createStore<ProjectsState>({
  projects: [],
  activeProjectId: null,
  loading: false,
});

export { projectsState };

export function setProjects(projects: ProjectEntry[]): void {
  setProjectsState("projects", projects);
}

export function setActiveProjectId(id: string | null): void {
  setProjectsState("activeProjectId", id);
}

export function setProjectsLoading(loading: boolean): void {
  setProjectsState("loading", loading);
}

/** Upsert a project into the in-memory list after opening / switching. */
export function upsertProject(entry: ProjectEntry): void {
  setProjectsState(
    produce((s) => {
      const idx = s.projects.findIndex((p) => p.id === entry.id);
      if (idx >= 0) {
        s.projects[idx] = entry;
      } else {
        s.projects.unshift(entry);
      }
    })
  );
}

/** Remove a project from the in-memory list. */
export function removeProjectFromStore(id: string): void {
  setProjectsState(
    produce((s) => {
      s.projects = s.projects.filter((p) => p.id !== id);
    })
  );
}

/** Toggle the pin flag in the in-memory list. */
export function setPinned(id: string, pinned: boolean): void {
  setProjectsState(
    produce((s) => {
      const entry = s.projects.find((p) => p.id === id);
      if (entry) entry.pinned = pinned;
      // Re-sort: pinned first, then by lastOpened desc.
      s.projects.sort((a, b) => {
        if (a.pinned !== b.pinned) return a.pinned ? -1 : 1;
        return b.lastOpened.localeCompare(a.lastOpened);
      });
    })
  );
}
