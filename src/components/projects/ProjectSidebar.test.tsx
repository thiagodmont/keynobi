import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen } from "@solidjs/testing-library";
import type { ProjectEntry } from "@/bindings";
import { setProjects } from "@/stores/projects.store";
import { setProjectState } from "@/stores/project.store";

const service = vi.hoisted(() => ({
  selectProject: vi.fn(() => Promise.resolve()),
  openProjectFolder: vi.fn(() => Promise.resolve(null)),
  removeProjectEntry: vi.fn(() => Promise.resolve()),
  renameProjectEntry: vi.fn(() => Promise.resolve()),
  trustProject: vi.fn(() => Promise.resolve()),
  revokeProjectTrust: vi.fn(() => Promise.resolve()),
}));
vi.mock("@/services/project.service", () => service);

import { ProjectSidebar } from "./ProjectSidebar";

function project(id: string, name: string, trusted = true): ProjectEntry {
  return {
    id,
    path: `/work/${name}`,
    name,
    gradleRoot: `/work/${name}`,
    lastOpened: "2026-09-01T00:00:00Z",
    pinned: false,
    lastBuildVariant: null,
    lastDevice: null,
    trusted,
  };
}

describe("ProjectSidebar from the keyboard", () => {
  beforeEach(() => {
    setProjects([project("a", "Shop"), project("b", "Wallet", false)]);
    setProjectState({ projectRoot: "/work/Shop", gradleRoot: null, projectName: "Shop" });
  });

  afterEach(() => {
    cleanup();
    vi.clearAllMocks();
    setProjects([]);
    setProjectState({ projectRoot: null, gradleRoot: null, projectName: null, loading: false });
  });

  function option(name: string): HTMLElement {
    return screen.getAllByRole("option").find((o) => o.textContent?.includes(name))!;
  }

  it("lists projects as options with the open one selected and focusable", () => {
    render(() => <ProjectSidebar />);
    expect(screen.getByRole("listbox", { name: "Projects" })).not.toBeNull();
    expect(option("Shop").getAttribute("aria-selected")).toBe("true");
    expect(option("Shop").tabIndex).toBe(0);
    expect(option("Wallet").tabIndex).toBe(-1);
  });

  it("opens a project with the arrow keys and Enter", () => {
    render(() => <ProjectSidebar />);
    option("Shop").focus();

    fireEvent.keyDown(option("Shop"), { key: "ArrowDown" });
    fireEvent.keyDown(document.activeElement!, { key: "Enter" });

    expect(service.selectProject).toHaveBeenCalledWith(expect.objectContaining({ id: "b" }));
  });

  it("Shift+F10 opens the row's actions, and Escape returns focus to the row", () => {
    render(() => <ProjectSidebar />);
    const wallet = option("Wallet");
    wallet.focus();

    fireEvent.keyDown(wallet, { key: "F10", shiftKey: true });

    const menu = screen.getByRole("menu", { name: "Actions for Wallet" });
    expect(
      Array.from(menu.querySelectorAll('[role="menuitem"]')).map((i) => i.textContent)
    ).toEqual(["Rename…", "Trust Project", "Remove from List"]);
    expect(document.activeElement?.textContent).toBe("Rename…");

    fireEvent.keyDown(document.activeElement!, { key: "Escape" });
    expect(screen.queryByRole("menu")).toBeNull();
    expect(document.activeElement).toBe(wallet);
  });

  it("runs a menu action from the keyboard", () => {
    render(() => <ProjectSidebar />);
    const wallet = option("Wallet");
    wallet.focus();
    fireEvent.keyDown(wallet, { key: "ContextMenu" });

    fireEvent.keyDown(document.activeElement!, { key: "ArrowDown" });
    fireEvent.keyDown(document.activeElement!, { key: "Enter" });

    expect(service.trustProject).toHaveBeenCalledWith(expect.objectContaining({ id: "b" }));
    expect(screen.queryByRole("menu")).toBeNull();
  });

  it("renames from the menu, then gives focus back to the row", async () => {
    render(() => <ProjectSidebar />);
    const wallet = option("Wallet");
    wallet.focus();
    fireEvent.keyDown(wallet, { key: "F10", shiftKey: true });
    fireEvent.keyDown(document.activeElement!, { key: "Enter" });

    const field = wallet.querySelector("input")!;
    expect(field.value).toBe("Wallet");
    fireEvent.input(field, { target: { value: "Wallet App" } });
    fireEvent.keyDown(field, { key: "Enter" });

    expect(service.renameProjectEntry).toHaveBeenCalledWith("b", "Wallet App");
    expect(service.selectProject).not.toHaveBeenCalled();
    expect(wallet.querySelector("input")).toBeNull();
    expect(document.activeElement).toBe(wallet);
  });
});
