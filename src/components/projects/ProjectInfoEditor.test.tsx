import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@solidjs/testing-library";
import { invoke } from "@tauri-apps/api/core";
import type { ProjectAppInfo } from "@/bindings";
import { clearProject, setProjectState } from "@/stores/project.store";
import {
  MAX_VERSION_CODE,
  ProjectInfoEditor,
  closeProjectInfoEditor,
  openProjectInfoEditor,
  validateProjectInfoInput,
} from "./ProjectInfoEditor";

const RANGE_MESSAGE = `Version code must be a whole number from 1 to ${MAX_VERSION_CODE}.`;

describe("validateProjectInfoInput", () => {
  it("accepts valid version info and trims the version name", () => {
    const result = validateProjectInfoInput(" 1.2.3 ", "42");

    expect(result).toEqual({ ok: true, versionName: "1.2.3", versionCode: 42 });
  });

  it("rejects partially numeric version codes", () => {
    expect(validateProjectInfoInput("1.2.3", "42beta")).toEqual({
      ok: false,
      message: RANGE_MESSAGE,
    });
  });

  it("rejects version codes outside what Android accepts", () => {
    expect(validateProjectInfoInput("1.2.3", "0")).toEqual({ ok: false, message: RANGE_MESSAGE });
    expect(validateProjectInfoInput("1.2.3", String(MAX_VERSION_CODE + 1))).toEqual({
      ok: false,
      message: RANGE_MESSAGE,
    });
    expect(validateProjectInfoInput("1.2.3", "9223372036854775808")).toEqual({
      ok: false,
      message: RANGE_MESSAGE,
    });
    expect(validateProjectInfoInput("1.2.3", String(MAX_VERSION_CODE))).toEqual({
      ok: true,
      versionName: "1.2.3",
      versionCode: MAX_VERSION_CODE,
    });
  });

  it("rejects version names that would break a Gradle string literal", () => {
    const result = validateProjectInfoInput('1.2"3', "42");

    expect(result).toEqual({
      ok: false,
      message: "Version name cannot contain quotes, backslashes, '$', or line breaks.",
    });
  });

  it("rejects version names with Gradle string interpolation syntax", () => {
    const result = validateProjectInfoInput("1.$0", "42");

    expect(result).toEqual({
      ok: false,
      message: "Version name cannot contain quotes, backslashes, '$', or line breaks.",
    });
  });

  it("leaves out a field that cannot be edited", () => {
    expect(validateProjectInfoInput(null, "7")).toEqual({
      ok: true,
      versionName: null,
      versionCode: 7,
    });
    expect(validateProjectInfoInput(null, null)).toEqual({
      ok: false,
      message: "Neither field can be edited here.",
    });
  });
});

describe("ProjectInfoEditor", () => {
  const editable: ProjectAppInfo = {
    applicationId: "com.example.app",
    versionName: "1.0.0",
    versionCode: 41,
    versionNameUnavailable: null,
    versionCodeUnavailable: null,
  };

  function stubBackend(info: ProjectAppInfo, save: () => Promise<void> = () => Promise.resolve()) {
    vi.mocked(invoke).mockImplementation((command: string) => {
      if (command === "get_project_app_info") return Promise.resolve(info);
      if (command === "save_project_app_info") return save();
      return Promise.reject(new Error(`unexpected command ${command}`));
    });
  }

  function saveCalls() {
    return vi.mocked(invoke).mock.calls.filter(([command]) => command === "save_project_app_info");
  }

  beforeEach(() => {
    vi.mocked(invoke).mockReset();
    setProjectState({ projectRoot: "/work/app", projectName: "App" });
  });

  afterEach(() => {
    closeProjectInfoEditor();
    cleanup();
    clearProject();
  });

  it("sends the version code as a JSON number", async () => {
    stubBackend(editable);
    render(() => <ProjectInfoEditor />);
    openProjectInfoEditor();

    const code = await screen.findByLabelText("Version Code");
    fireEvent.input(code, { target: { value: "42" } });
    fireEvent.click(screen.getByRole("button", { name: "Save" }));

    await waitFor(() => expect(saveCalls()).toHaveLength(1));
    const args = saveCalls()[0][1];
    expect(args).toEqual({ versionName: "1.0.0", versionCode: 42 });
    expect(JSON.parse(JSON.stringify(args))).toEqual({ versionName: "1.0.0", versionCode: 42 });
  });

  it("shows a rejected save in the dialog and stays open", async () => {
    stubBackend(editable, () =>
      Promise.reject({
        kind: "invalidInput",
        message: "app/build.gradle.kts already has these values; nothing was changed",
      })
    );
    render(() => <ProjectInfoEditor />);
    openProjectInfoEditor();

    await screen.findByLabelText("Version Code");
    fireEvent.click(screen.getByRole("button", { name: "Save" }));

    const alert = await screen.findByRole("alert");
    expect(alert.textContent).toContain("nothing was changed");
    expect(screen.getByRole("dialog")).toBeTruthy();
  });

  it("shows why a field defined elsewhere cannot be edited and leaves it out of the save", async () => {
    stubBackend({
      ...editable,
      versionCode: null,
      versionCodeUnavailable:
        "versionCode in app/build.gradle.kts (line 7) is set by `libs.versions.code.get().toInt()`",
    });
    render(() => <ProjectInfoEditor />);
    openProjectInfoEditor();

    await screen.findByText(/is set by `libs\.versions\.code\.get\(\)\.toInt\(\)`/);
    expect(screen.queryByLabelText("Version Code")).toBeNull();

    fireEvent.input(screen.getByLabelText("Version Name"), { target: { value: "1.1.0" } });
    fireEvent.click(screen.getByRole("button", { name: "Save" }));

    await waitFor(() => expect(saveCalls()).toHaveLength(1));
    expect(saveCalls()[0][1]).toEqual({ versionName: "1.1.0", versionCode: null });
  });

  it("reports an out-of-range version code without calling the backend", async () => {
    stubBackend(editable);
    render(() => <ProjectInfoEditor />);
    openProjectInfoEditor();

    const code = await screen.findByLabelText("Version Code");
    fireEvent.input(code, { target: { value: String(MAX_VERSION_CODE + 1) } });
    fireEvent.click(screen.getByRole("button", { name: "Save" }));

    expect((await screen.findByRole("alert")).textContent).toContain(RANGE_MESSAGE);
    expect(saveCalls()).toHaveLength(0);
  });
});
