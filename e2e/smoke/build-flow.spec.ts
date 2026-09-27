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

deployTest(
  "a run whose install fails says why, launches nothing, and can run again",
  async ({ page }) => {
    await selectMockProject(page);
    await page.getByRole("tab", { name: "Build" }).click();
    await page.evaluate(() =>
      window.__e2e__.failNextInstall("adb: failed to install: INSTALL_FAILED_INSUFFICIENT_STORAGE")
    );
    const runApp = page.getByTitle(/^Run App/).first();
    await runApp.click();

    await expect(
      page.getByText(/Deploy failed: .*INSTALL_FAILED_INSUFFICIENT_STORAGE/)
    ).toBeVisible({
      timeout: 10_000,
    });
    // The install phase logged its steps; nothing was launched.
    await expect(page.getByText(/^▶ adb install .*app-debug\.apk$/)).toBeVisible();
    await expect(page.getByText(/^▶ adb shell am start/)).toHaveCount(0);
    await expect(page.getByText("Installing APK…")).toHaveCount(0);

    // The failed run is over, so the next one installs and launches.
    await expect(runApp).toBeEnabled();
    await runApp.click();
    await expect(page.getByText("▶ Launch time: 812 ms (cold) · displayed 790 ms")).toBeVisible({
      timeout: 10_000,
    });
  }
);

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

test("an agent's run shows its install and launch under its build, without taking it over", async ({
  page,
}) => {
  await selectMockProject(page);
  await page.getByRole("tab", { name: "Build" }).click();

  await page.evaluate(() =>
    window.__e2e__.startAgentRun("Default", "emulator-5554", "Claude Code")
  );

  await expect(page.getByText(/Started by an agent \(Claude Code\)/).first()).toBeVisible();
  await expect(page.getByText(/^▶ adb install .*app-debug\.apk$/)).toBeVisible();
  await expect(page.getByText(/^▶ adb shell am start -W/)).toBeVisible();
  await expect(
    page.getByText(/^▶ Run 'Default' by an agent \(Claude Code\): done on /)
  ).toBeVisible();
  // The app shows the run; it does not follow it as its own.
  await expect(page.getByText(/^▶ Launch: /)).toHaveCount(0);
  await expect(page.getByTitle(/^Run App/).first()).toBeEnabled();
});

test("a configuration shared with the project is written to its file and approved before it builds", async ({
  page,
}) => {
  await selectMockProject(page);
  const picker = page.getByRole("combobox", { name: "Run configuration" });
  await expect(picker).toHaveValue("Default", { timeout: 5_000 });

  await picker.selectOption({ label: "Edit Configurations…" });
  const editor = page.getByRole("dialog", { name: "Run Configurations" });
  await editor.getByRole("button", { name: "Add" }).click();
  await editor.getByLabel("Name", { exact: true }).fill("Bundle");
  await editor.getByLabel("Gradle task", { exact: true }).fill(":app:bundleDebug");
  await editor.getByRole("checkbox", { name: "Share with project" }).check();
  await editor.getByRole("button", { name: "Save" }).click();

  await expect(
    editor.getByRole("listbox", { name: "Run configurations" }).getByRole("option", {
      name: /Bundle/,
    })
  ).toContainText("Shared");
  const file = await page.evaluate(() => window.__e2e__.sharedRunConfigurationsFile());
  expect(JSON.parse(file ?? "null")).toEqual({
    schemaVersion: 1,
    configurations: [
      {
        name: "Bundle",
        module: ":app",
        variant: "debug",
        task: ":app:bundleDebug",
        logcatFilter: "package:mine",
      },
    ],
  });
  // A task outside assemble* needs approval before it runs.
  await expect(editor.getByText(/You have not approved it yet/)).toBeVisible();
  await editor.getByRole("button", { name: "Close" }).click();
  await expect(editor).toBeHidden();

  await picker.selectOption("Bundle");
  await page.getByRole("tab", { name: "Build" }).click();
  await page.getByTitle(/Build only/i).click();
  const approval = page.getByRole("dialog", { name: "Approve shared run configuration?" });
  await expect(approval).toContainText("builds :app:bundleDebug, which is not an assemble task");
  await approval.getByRole("button", { name: "Approve" }).click();

  await expect(page.getByText("Build 'Bundle': build :app:bundleDebug")).toBeVisible({
    timeout: 10_000,
  });
  await expect(page.getByText(/BUILD SUCCESSFUL in 4s/i)).toBeVisible({ timeout: 10_000 });

  // A pulled change to the file asks again.
  await page.evaluate(() =>
    window.__e2e__.setSharedRunConfigurationsFile(
      (window.__e2e__.sharedRunConfigurationsFile() ?? "").replace("bundleDebug", "bundleRelease")
    )
  );
  await page.getByTitle(/Build only/i).click();
  await expect(approval).toContainText("The file changed since you approved it.");
  await approval.getByRole("button", { name: "Cancel" }).click();
  await expect(
    page.getByText("The shared run configuration was not approved — build cancelled.")
  ).toBeVisible();
});
