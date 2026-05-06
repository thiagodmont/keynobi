import { fireEvent, render, screen } from "@solidjs/testing-library";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { settingsState, updateSetting } from "@/stores/settings.store";
import { openSettings, closeSettings, SettingsPanel } from "./SettingsPanel";

vi.mock("@tauri-apps/api/app", () => ({
  getVersion: vi.fn().mockResolvedValue("0.0.0-test"),
}));

describe("SettingsPanel", () => {
  beforeEach(() => {
    closeSettings();
    updateSetting("logcat", "outputFontSize", 11);
  });

  it("lets the user change the Logcat output font size", async () => {
    render(() => <SettingsPanel />);
    openSettings();

    fireEvent.click(screen.getByRole("button", { name: /tools/i }));
    fireEvent.input(screen.getByPlaceholderText("Search settings..."), {
      target: { value: "output font" },
    });

    expect(await screen.findByText("Logcat Output Font Size")).not.toBeNull();
    const input = screen.getByDisplayValue("11");

    fireEvent.input(input, { target: { value: "13" } });

    expect(settingsState.logcat.outputFontSize).toBe(13);
  });
});
