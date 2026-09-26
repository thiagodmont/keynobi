import { test as base } from "@playwright/test";
import { test, expect } from "../fixtures/app";

async function selectMockProject(page: import("@playwright/test").Page): Promise<void> {
  await page.getByText("MockProject", { exact: true }).first().click();
  await expect(page.getByText("Keynobi — MockProject")).toBeVisible({ timeout: 5_000 });
}

test("build tab is reachable and visible", async ({ page }) => {
  const buildTab = page.getByRole("tab", { name: "Build" });
  await expect(buildTab).toBeVisible({ timeout: 5_000 });
  await buildTab.click();
  await expect(buildTab).toHaveAttribute("aria-selected", "true");
});

test("running a build shows build lines then success indicator", async ({ page }) => {
  await selectMockProject(page);

  const buildTab = page.getByRole("tab", { name: "Build" });
  await buildTab.click();

  const buildOnlyButton = page.getByTitle(/Build only/i);
  await expect(buildOnlyButton).toBeVisible({ timeout: 5_000 });
  await buildOnlyButton.click();

  await expect(page.getByText(/BUILD SUCCESSFUL in 4s/i)).toBeVisible({ timeout: 10_000 });
});

// Run App installs and launches only when Auto Install on Build is on.
const deployTest = base.extend({
  page: async ({ page }, use) => {
    await page.addInitScript(() => {
      (
        window as Window & { __keynobi_e2e_settings_overrides?: Record<string, unknown> }
      ).__keynobi_e2e_settings_overrides = {
        build: {
          autoInstallOnBuild: true,
          autoScrollBuildLog: true,
          buildLogRetentionDays: 7,
          buildLogMaxFolderMb: 100,
        },
      };
    });
    await page.goto("/");
    await page.waitForFunction(() => typeof window.__e2e__ !== "undefined", { timeout: 10_000 });
    await use(page);
  },
});

deployTest("Run App records the launch time on the build it installed", async ({ page }) => {
  await selectMockProject(page);
  await page.getByRole("tab", { name: "Build" }).click();

  // The mock project's emulator is online and selected, so no device prompt.
  await page
    .getByTitle(/^Run App/)
    .first()
    .click();

  await expect(page.getByText("▶ Launch time: 812 ms (cold) · displayed 790 ms")).toBeVisible({
    timeout: 10_000,
  });
  // The fully drawn time arrives after the launch returned (build:launch_timing).
  await expect(page.getByTestId("launch-timing").first()).toHaveText(
    "Launch 812 ms (cold) · displayed 790 ms · fully drawn 1.4 s",
    { timeout: 10_000 }
  );
});

deployTest(
  "Run App builds the application module and installs the APK that build wrote",
  async ({ page }) => {
    await selectMockProject(page);
    await page.getByRole("tab", { name: "Build" }).click();
    await page
      .getByTitle(/^Run App/)
      .first()
      .click();

    await expect(
      page.getByText(
        "Run 'Default': build :app:assembleDebug → install this build's APK → launch the app on Pixel_6_API_34 → filter package:mine"
      )
    ).toBeVisible({ timeout: 10_000 });
    await expect(page.getByText(/^▶ APK \(build #\d+\): .*app-debug\.apk$/)).toBeVisible({
      timeout: 10_000,
    });
    const history = (await page.evaluate(() => window.__e2e__.invoke("get_build_history"))) as {
      task: string;
    }[];
    expect(history.map((record) => record.task)).toContain(":app:assembleDebug");
  }
);

/** Save a configuration of the mock project and make it the active one. */
async function activateConfiguration(
  page: import("@playwright/test").Page,
  config: Record<string, unknown>
): Promise<void> {
  await page.evaluate(async (config) => {
    await window.__e2e__.invoke("save_run_configuration", { config });
    await window.__e2e__.invoke("set_active_run_configuration", { name: config.name });
  }, config);
}

const settingsConfiguration = {
  name: "Settings",
  module: ":app",
  variant: "release",
  task: null,
  launch: { kind: "activity", name: ".SettingsActivity" },
  logcatFilter: "package:mine level:warn",
};

deployTest("Run App runs the active run configuration and shows its plan", async ({ page }) => {
  await selectMockProject(page);
  await activateConfiguration(page, settingsConfiguration);
  await page.getByRole("tab", { name: "Build" }).click();

  await page
    .getByTitle(/^Run App/)
    .first()
    .click();

  await expect(
    page.getByText(
      "Run 'Settings': build :app:assembleRelease → install this build's APK → launch .SettingsActivity on Pixel_6_API_34 → filter package:mine level:warn"
    )
  ).toBeVisible({ timeout: 10_000 });
  await expect(page.getByText(/^▶ APK \(build #\d+\): .*app-release\.apk$/)).toBeVisible({
    timeout: 10_000,
  });
  await expect(
    page.getByText("▶ adb shell am start -W (com.example.mockapp.debug/.SettingsActivity)")
  ).toBeVisible({ timeout: 10_000 });
  const history = (await page.evaluate(() => window.__e2e__.invoke("get_build_history"))) as {
    task: string;
  }[];
  expect(history.map((record) => record.task)).toContain(":app:assembleRelease");
  // The run remembers the device it installed on.
  const configurations = (await page.evaluate(() =>
    window.__e2e__.invoke("list_run_configurations")
  )) as { local: Record<string, { lastDevice: string | null }> };
  expect(configurations.local.Settings?.lastDevice).toBe("emulator-5554");
});

test("Build Only builds the active run configuration's task", async ({ page }) => {
  await selectMockProject(page);
  await activateConfiguration(page, settingsConfiguration);
  await page.getByRole("tab", { name: "Build" }).click();

  await page.getByTitle(/Build only/i).click();

  await expect(page.getByText("Build 'Settings': build :app:assembleRelease")).toBeVisible({
    timeout: 10_000,
  });
  await expect(page.getByText(/BUILD SUCCESSFUL in 4s/i)).toBeVisible({ timeout: 10_000 });
  const history = (await page.evaluate(() => window.__e2e__.invoke("get_build_history"))) as {
    task: string;
  }[];
  expect(history.map((record) => record.task)).toEqual([":app:assembleRelease"]);
});

deployTest(
  "a configuration made in the editor is picked in the title bar and run",
  async ({ page }) => {
    await selectMockProject(page);
    const picker = page.getByRole("combobox", { name: "Run configuration" });
    await expect(picker).toHaveValue("Default", { timeout: 5_000 });

    await picker.selectOption({ label: "Edit Configurations…" });
    const editor = page.getByRole("dialog", { name: "Run Configurations" });
    await expect(editor).toBeVisible();
    await expect(picker).toHaveValue("Default");
    await editor.getByRole("button", { name: "Add" }).click();
    await editor.getByLabel("Name", { exact: true }).fill("Release run");
    await editor.getByLabel("Variant", { exact: true }).selectOption("release");
    await expect(editor.getByLabel("Gradle task", { exact: true })).toHaveValue(
      ":app:assembleRelease"
    );
    await editor.getByLabel("Launch", { exact: true }).selectOption("activity");
    await editor.getByLabel("Activity", { exact: true }).fill(".SettingsActivity");
    await editor.getByLabel("Logcat filter", { exact: true }).fill("package:mine level:warn");
    await editor.getByRole("button", { name: "Save" }).click();

    await expect(editor.getByTestId("run-config-plan")).toContainText(
      "Run 'Release run': build :app:assembleRelease"
    );
    await editor.getByRole("button", { name: "Close" }).click();
    await expect(editor).toBeHidden();

    await picker.selectOption("Release run");
    await expect(picker).toHaveValue("Release run");
    await page.getByRole("tab", { name: "Build" }).click();
    await page
      .getByTitle(/^Run App/)
      .first()
      .click();

    await expect(
      page.getByText(
        "Run 'Release run': build :app:assembleRelease → install this build's APK → launch .SettingsActivity on Pixel_6_API_34 → filter package:mine level:warn"
      )
    ).toBeVisible({ timeout: 10_000 });
    await expect(
      page.getByText("▶ adb shell am start -W (com.example.mockapp.debug/.SettingsActivity)")
    ).toBeVisible({ timeout: 10_000 });
  }
);

test("the command palette runs, builds, and edits run configurations", async ({ page }) => {
  await selectMockProject(page);
  await expect(page.getByRole("combobox", { name: "Run configuration" })).toHaveValue("Default", {
    timeout: 5_000,
  });

  const palette = page.getByRole("dialog", { name: "Command Palette" });
  await page.keyboard.press("Meta+Shift+P");
  await page.keyboard.type("Build: Default");
  await expect(page.getByRole("option", { name: /Run: Default/ })).toBeHidden();
  await expect(page.getByRole("option", { name: /Build: Default/ })).toBeVisible();
  await page.keyboard.press("Enter");
  await expect(palette).toBeHidden();
  await page.getByRole("tab", { name: "Build" }).click();
  await expect(page.getByText("Build 'Default': build :app:assembleDebug")).toBeVisible({
    timeout: 10_000,
  });

  await page.keyboard.press("Meta+Shift+P");
  await page.keyboard.type("Edit Run Configurations");
  await expect(page.getByRole("option", { name: /Edit Run Configurations…/ })).toBeVisible();
  await page.keyboard.press("Enter");
  await expect(page.getByRole("dialog", { name: "Run Configurations" })).toBeVisible();
});

deployTest("a past build says which device Run App installed it on", async ({ page }) => {
  await selectMockProject(page);
  await page.getByRole("tab", { name: "Build" }).click();
  await page
    .getByTitle(/^Run App/)
    .first()
    .click();
  await expect(page.getByText("▶ Launch time: 812 ms (cold)")).toBeVisible({ timeout: 10_000 });

  await page.getByRole("listbox", { name: "Builds" }).getByRole("option").first().click();

  await expect(page.getByText(/^Viewing build #\d+ from /)).toBeVisible();
  await expect(page.getByText(/^Installed on Pixel_6_API_34 · /)).toBeVisible();
});

test("an agent's build shows who started it and can be cancelled from the app", async ({
  page,
}) => {
  await selectMockProject(page);
  await page.getByRole("tab", { name: "Build" }).click();

  // Slow enough to act on while it runs.
  await page.evaluate(() => window.__e2e__.startAgentBuild("assembleDebug", "Claude Code", 5_000));

  await expect(page.getByText(/Started by an agent \(Claude Code\)/).first()).toBeVisible({
    timeout: 5_000,
  });
  await expect(
    page.getByTitle("A build started by an agent (Claude Code) is running")
  ).toBeDisabled();

  await page.getByTitle("Cancel the build started by an agent (Claude Code)").first().click();

  await expect(
    page.getByText("Build cancelled · Started by an agent (Claude Code) · Cancelled in Keynobi")
  ).toBeVisible({ timeout: 5_000 });
});
