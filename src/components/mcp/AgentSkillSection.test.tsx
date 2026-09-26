import { cleanup, fireEvent, render, screen, waitFor } from "@solidjs/testing-library";
import { invoke } from "@tauri-apps/api/core";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { AgentSkillState, AgentSkillStatus } from "@/bindings";
import { DialogHost } from "@/components/ui";
import { resetDialogHostForTests } from "@/components/ui/Dialog/Dialog.test-utils";
import { AgentSkillSection } from "./AgentSkillSection";

const SKILL_PATH = "/Users/me/.claude/skills/keynobi/SKILL.md";
const CONTENT = "---\nname: keynobi\ndescription: When to use Keynobi.\n---\n";

function skill(state: AgentSkillState): AgentSkillStatus {
  return { path: SKILL_PATH, state, content: CONTENT, resourceUri: "keynobi://skill" };
}

/** A backend whose skill starts in `initial`; install moves it to `installed`. */
function withSkill(initial: AgentSkillState): void {
  vi.mocked(invoke).mockImplementation(async (cmd: string) => {
    if (cmd === "get_agent_skill_status") return skill(initial);
    if (cmd === "install_agent_skill") return skill("installed");
    throw new Error(`unexpected ${cmd}`);
  });
}

function installCalls(): unknown[] {
  return vi
    .mocked(invoke)
    .mock.calls.filter(([cmd]) => cmd === "install_agent_skill")
    .map(([, args]) => args);
}

function renderSection(): void {
  render(() => (
    <>
      <DialogHost />
      <AgentSkillSection />
    </>
  ));
}

describe("AgentSkillSection", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    resetDialogHostForTests();
  });

  afterEach(() => {
    cleanup();
  });

  it("shows where the skill goes and writes nothing until the user clicks", async () => {
    withSkill("notInstalled");
    renderSection();

    expect(await screen.findByText(SKILL_PATH)).toBeTruthy();
    expect(screen.getByText("Not installed")).toBeTruthy();
    expect(screen.getByText("keynobi://skill")).toBeTruthy();
    expect(installCalls()).toEqual([]);

    fireEvent.click(screen.getByRole("button", { name: "Show SKILL.md" }));
    expect(screen.getByLabelText("SKILL.md").textContent).toBe(CONTENT);
    expect(installCalls()).toEqual([]);

    fireEvent.click(screen.getByRole("button", { name: "Install for Claude Code" }));

    await waitFor(() => expect(installCalls()).toEqual([{ replace: false }]));
    expect(await screen.findByText("Installed")).toBeTruthy();
    expect(screen.queryByRole("button", { name: /for Claude Code/ })).toBeNull();
  });

  it("replaces a different SKILL.md only after the user confirms", async () => {
    withSkill("different");
    renderSection();

    const replace = await screen.findByRole("button", { name: "Replace for Claude Code…" });
    fireEvent.click(replace);
    fireEvent.click(await screen.findByRole("button", { name: "Cancel" }));
    await waitFor(() => expect(screen.queryByText("Replace SKILL.md?")).toBeNull());
    expect(installCalls()).toEqual([]);

    fireEvent.click(replace);
    expect(await screen.findByText("Replace SKILL.md?")).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "Replace" }));

    await waitFor(() => expect(installCalls()).toEqual([{ replace: true }]));
    expect(await screen.findByText("Installed")).toBeTruthy();
  });

  it("copies the skill for other clients", async () => {
    withSkill("installed");
    const writeText = vi.fn().mockResolvedValue(undefined);
    Object.defineProperty(navigator, "clipboard", { value: { writeText }, configurable: true });
    renderSection();

    fireEvent.click(await screen.findByRole("button", { name: "Copy SKILL.md" }));

    expect(writeText).toHaveBeenCalledWith(CONTENT);
    expect(installCalls()).toEqual([]);
  });
});
