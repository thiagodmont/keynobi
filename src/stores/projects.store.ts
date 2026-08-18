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

/** Sort order for the sidebar: pinned first, then most-recently-opened. */
function sortProjects(list: ProjectEntry[]): void {
  list.sort((a, b) => {
    if (a.pinned !== b.pinned) return a.pinned ? -1 : 1;
    return b.lastOpened.localeCompare(a.lastOpened);
  });
}

/** Upsert a project into the in-memory list after opening / switching. */
export function upsertProject(entry: ProjectEntry): void {
  setProjectsState(
    produce((s) => {
      const idx = s.projects.findIndex((p) => p.id === entry.id);
      if (idx >= 0) {
        s.projects[idx] = entry;
      } else {
        s.projects.push(entry);
      }
      // Re-sort rather than unshifting: an unshifted unpinned project would sit
      // above pinned ones until the next setPinned call.
      sortProjects(s.projects);
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
      sortProjects(s.projects);
    })
  );
}

/** Update the display name in the in-memory list. */
export function renameProjectInStore(id: string, newName: string): void {
  setProjectsState(
    produce((s) => {
      const entry = s.projects.find((p) => p.id === id);
      if (entry) entry.name = newName;
    })
  );
}

/** Update per-project meta (variant / device) in the in-memory list. */
export function updateProjectMetaInStore(
  id: string,
  lastBuildVariant: string | null,
  lastDevice: string | null
): void {
  setProjectsState(
    produce((s) => {
      const entry = s.projects.find((p) => p.id === id);
      if (entry) {
        entry.lastBuildVariant = lastBuildVariant;
        entry.lastDevice = lastDevice;
      }
    })
  );
}
