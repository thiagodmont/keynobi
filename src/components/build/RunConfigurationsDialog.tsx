import {
  type JSX,
  Show,
  createEffect,
  createMemo,
  createResource,
  createSignal,
  on,
  untrack,
} from "solid-js";
import { Portal } from "solid-js/web";
import {
  Alert,
  Badge,
  Button,
  FormField,
  Input,
  Listbox,
  ScrollArea,
  Select,
  type SelectOption,
  modalFocus,
  showDialog,
} from "@/components/ui";
import type { RunConfiguration, TargetPreference } from "@/bindings";
import {
  errorMessage,
  formatError,
  getVariantsPreview,
  isAppErrorKind,
  listApplicationModules,
} from "@/lib/tauri-api";
import { runConfigState } from "@/stores/run-configurations.store";
import { projectState } from "@/stores/project.store";
import { deviceState } from "@/stores/device.store";
import { variantState } from "@/stores/variant.store";
import {
  closeRunConfigurationsEditor,
  copyName,
  deleteRunConfiguration,
  launchRunAvd,
  previewRunPlan,
  saveRunConfiguration,
} from "@/services/run-configurations.service";
import {
  type RunConfigurationDraft,
  configurationFrom,
  defaultTask,
  draftChanged,
  draftFrom,
  parseTarget,
  validateDraft,
} from "./run-configuration-draft";
import styles from "./RunConfigurationsDialog.module.css";

const LAUNCH_OPTIONS: SelectOption[] = [
  { label: "Default activity", value: "default" },
  { label: "Activity", value: "activity" },
  { label: "Deep link", value: "deepLink" },
  { label: "Nothing (install only)", value: "none" },
];

function savedTarget(name: string): TargetPreference {
  return runConfigState.local[name]?.target ?? { kind: "lastUsed" };
}

function savedDraft(name: string): RunConfigurationDraft | null {
  const config = runConfigState.configurations.find((c) => c.name === name);
  return config ? draftFrom(config, savedTarget(name)) : null;
}

/** A name no configuration has: `Configuration 2`. */
function newName(): string {
  const taken = new Set(runConfigState.configurations.map((c) => c.name.toLowerCase()));
  for (let i = 1; ; i++) {
    const name = i === 1 ? "Configuration" : `Configuration ${i}`;
    if (!taken.has(name.toLowerCase())) return name;
  }
}

/** Variant names of `module`: the variant picker's when it lists that module. */
async function variantsOf(module: string, onlyModule: boolean): Promise<string[]> {
  if ((onlyModule || variantState.module === module) && variantState.variants.length) {
    return variantState.variants.map((v) => v.name);
  }
  const list = await getVariantsPreview(onlyModule ? null : module);
  return list.variants.map((v) => v.name);
}

/** Edit Run Configurations: add, duplicate, delete, and edit the open project's configurations. */
export function RunConfigurationsDialog(): JSX.Element {
  const [selected, setSelected] = createSignal<string | null>(
    untrack(() => runConfigState.active ?? runConfigState.configurations[0]?.name ?? null)
  );
  const [draft, setDraft] = createSignal<RunConfigurationDraft | null>(null);
  const [showErrors, setShowErrors] = createSignal(false);
  const [saveError, setSaveError] = createSignal<string | null>(null);
  const [saving, setSaving] = createSignal(false);
  const [modules] = createResource(() => listApplicationModules().catch(() => [] as string[]));

  const saved = createMemo(() => {
    const name = selected();
    return name ? savedDraft(name) : null;
  });
  const dirty = () => {
    const current = draft();
    return !!current && draftChanged(current, saved());
  };
  const errors = createMemo(() => {
    const current = draft();
    return current ? validateDraft(current) : {};
  });
  const shownErrors = () => (showErrors() ? errors() : { logcatFilter: errors().logcatFilter });

  // Load the selected configuration into the form.
  createEffect(
    on(selected, (name) => {
      setShowErrors(false);
      setSaveError(null);
      const next = name ? savedDraft(name) : null;
      if (next || name !== null) setDraft(next);
    })
  );

  // A project switch closes the editor.
  createEffect(on(() => projectState.projectRoot, closeRunConfigurationsEditor, { defer: true }));

  const onlyModule = () => (modules()?.length ?? 0) <= 1;
  const [variants] = createResource(
    () => draft()?.module,
    (module) => variantsOf(module, onlyModule()).catch(() => [] as string[])
  );

  const moduleOptions = createMemo<string[]>(() => {
    const list = [...(modules() ?? [])];
    const current = draft()?.module;
    if (current && !list.includes(current)) list.push(current);
    return list;
  });

  const variantOptions = createMemo<string[]>(() => {
    const list = [...(variants() ?? [])];
    const current = draft()?.variant;
    if (current && !list.includes(current)) list.push(current);
    return list;
  });

  const targetOptions = createMemo<SelectOption[]>(() => {
    const options: SelectOption[] = [
      { label: "Ask each time", value: "ask" },
      { label: "Last used device", value: "lastUsed" },
    ];
    for (const device of deviceState.devices) {
      if (device.connectionState !== "online") continue;
      options.push({
        label: `Device: ${device.model ?? device.name} (${device.serial})`,
        value: `serial:${device.serial}`,
      });
    }
    for (const avd of deviceState.avds) {
      options.push({ label: `AVD: ${avd.displayName}`, value: `avd:${avd.name}` });
    }
    const current = draft()?.target;
    if (current && !options.some((o) => typeof o !== "string" && o.value === current)) {
      const target = parseTarget(current);
      options.push({
        label:
          target.kind === "serial"
            ? `Device: ${target.serial} (offline)`
            : target.kind === "avd"
              ? `AVD: ${target.name}`
              : current,
        value: current,
      });
    }
    return options;
  });

  function update(patch: Partial<RunConfigurationDraft>): void {
    const current = draft();
    if (!current) return;
    const next = { ...current, ...patch };
    // The task follows the module and variant until it is changed.
    if (
      (patch.module !== undefined || patch.variant !== undefined) &&
      current.task === defaultTask(current.module, current.variant)
    ) {
      next.task = defaultTask(next.module, next.variant);
    }
    setDraft(next);
    setSaveError(null);
  }

  /** Leave the draft; asks first when it has unsaved changes. */
  async function leaveDraft(): Promise<boolean> {
    if (!dirty()) return true;
    const choice = await showDialog({
      title: "Discard changes?",
      message: `Your changes to '${draft()?.name ?? ""}' are not saved.`,
      buttons: [
        { label: "Discard", value: "discard", style: "danger" },
        { label: "Keep Editing", value: "cancel", style: "secondary" },
      ],
    });
    return choice === "discard";
  }

  async function select(name: string): Promise<void> {
    if (name === selected() && draft()?.savedName === name) return;
    if (!(await leaveDraft())) return;
    if (name === selected()) {
      setDraft(savedDraft(name));
    } else {
      setSelected(name);
    }
  }

  async function startDraft(next: RunConfigurationDraft): Promise<void> {
    if (!(await leaveDraft())) return;
    setSelected(null);
    setDraft(next);
    setShowErrors(false);
    setSaveError(null);
  }

  async function add(): Promise<void> {
    const module = modules()?.[0] ?? runConfigState.configurations[0]?.module ?? ":app";
    const variant =
      variantState.activeVariant ?? runConfigState.configurations[0]?.variant ?? "debug";
    const config: RunConfiguration = {
      name: newName(),
      module,
      variant,
      task: null,
      launch: { kind: "default" },
      logcatFilter: "package:mine",
    };
    await startDraft(draftFrom(config, { kind: "lastUsed" }, null));
  }

  async function duplicate(): Promise<void> {
    const current = draft();
    if (!current) return;
    const names = runConfigState.configurations.map((c) => c.name);
    await startDraft({ ...current, savedName: null, name: copyName(current.name, names) });
  }

  async function remove(): Promise<void> {
    const current = draft();
    if (!current) return;
    if (current.savedName === null) {
      setDraft(null);
      setSelected(runConfigState.active ?? runConfigState.configurations[0]?.name ?? null);
      return;
    }
    const name = current.savedName;
    const choice = await showDialog({
      title: "Delete run configuration?",
      message: `Delete '${name}'? This cannot be undone.`,
      buttons: [
        { label: "Delete", value: "delete", style: "danger" },
        { label: "Cancel", value: "cancel", style: "secondary" },
      ],
    });
    if (choice !== "delete") return;
    try {
      await deleteRunConfiguration(name);
      setDraft(null);
      setSelected(runConfigState.active ?? runConfigState.configurations[0]?.name ?? null);
    } catch (e) {
      setSaveError(formatError(e));
    }
  }

  async function save(): Promise<void> {
    const current = draft();
    if (!current) return;
    setShowErrors(true);
    if (Object.keys(errors()).length > 0) return;
    const config = configurationFrom(current);
    setSaving(true);
    setSaveError(null);
    try {
      await saveRunConfiguration(config, parseTarget(current.target), current.savedName);
      setShowErrors(false);
      if (selected() === config.name) {
        setDraft(savedDraft(config.name));
      } else {
        setSelected(config.name);
      }
    } catch (e) {
      setSaveError(formatError(e));
    } finally {
      setSaving(false);
    }
  }

  async function close(): Promise<void> {
    if (await leaveDraft()) closeRunConfigurationsEditor();
  }

  // What Run App would do with the saved configuration now.
  const [plan, { refetch: refetchPlan }] = createResource(
    () => {
      const name = saved()?.name;
      if (!name) return null;
      // Changes with what the plan depends on: the saved configuration and the devices.
      const devices = deviceState.devices.map((d) => `${d.serial}:${d.connectionState}`);
      return JSON.stringify([name, saved(), devices, deviceState.selectedSerial]);
    },
    async (key: string) => {
      const [name] = JSON.parse(key) as [string];
      try {
        return { ok: true as const, text: (await previewRunPlan(name)).plan };
      } catch (e) {
        return { ok: false as const, error: e, text: errorMessage(e) };
      }
    }
  );

  /** The AVD to offer launching when the plan found it not running. */
  const avdToLaunch = () => {
    const result = plan();
    const current = saved();
    if (!result || result.ok || !current || !isAppErrorKind(result.error, "notFound")) return null;
    const target = parseTarget(current.target);
    return target.kind === "avd" ? target.name : null;
  };

  async function launchAvd(name: string): Promise<void> {
    await launchRunAvd(name);
    void refetchPlan();
  }

  const fieldId = (field: string) => `run-config-${field}`;

  return (
    <Portal>
      <div class={styles.backdrop} onClick={() => void close()}>
        <div
          ref={(el) => modalFocus(el, { onEscape: () => void close() })}
          class={styles.box}
          role="dialog"
          aria-modal="true"
          aria-labelledby="run-configurations-title"
          data-testid="run-configurations-dialog"
          onClick={(e) => e.stopPropagation()}
        >
          <h2 id="run-configurations-title" class={styles.title}>
            Run Configurations
          </h2>
          <div class={styles.body}>
            <div class={styles.sidebar}>
              <ScrollArea class={styles.list}>
                <Listbox
                  items={runConfigState.configurations}
                  label="Run configurations"
                  getKey={(c) => c.name}
                  isSelected={(c) => c.name === selected()}
                  onSelect={(c) => void select(c.name)}
                  testId="run-configurations-list"
                >
                  {(config) => (
                    <div class={styles.item}>
                      <span class={styles.itemName}>{config().name}</span>
                      <Show when={config().name === runConfigState.active}>
                        <Badge variant="accent" size="xs">
                          Active
                        </Badge>
                      </Show>
                    </div>
                  )}
                </Listbox>
                <Show when={draft() && draft()?.savedName === null}>
                  <div class={styles.newItem}>
                    {draft()?.name || "New configuration"} (not saved)
                  </div>
                </Show>
              </ScrollArea>
              <div class={styles.listActions}>
                <Button size="xs" onClick={() => void add()}>
                  Add
                </Button>
                <Button size="xs" disabled={!draft()} onClick={() => void duplicate()}>
                  Duplicate
                </Button>
                <Button
                  size="xs"
                  variant="danger"
                  disabled={!draft()}
                  onClick={() => void remove()}
                >
                  Delete
                </Button>
              </div>
            </div>
            <Show
              when={draft()}
              fallback={
                <div class={styles.empty}>
                  Add a configuration to choose what Run App builds and launches.
                </div>
              }
            >
              {(current) => (
                <form
                  class={styles.form}
                  aria-label="Run configuration"
                  onSubmit={(e) => {
                    e.preventDefault();
                    void save();
                  }}
                >
                  <FormField id={fieldId("name")} label="Name" error={shownErrors().name} required>
                    <Input
                      id={fieldId("name")}
                      size="sm"
                      value={current().name}
                      state={shownErrors().name ? "error" : "default"}
                      onInput={(name) => update({ name })}
                    />
                  </FormField>
                  <div class={styles.row}>
                    <FormField id={fieldId("module")} label="Module">
                      <Select
                        id={fieldId("module")}
                        value={current().module}
                        options={moduleOptions()}
                        onChange={(module) => update({ module })}
                      />
                    </FormField>
                    <FormField id={fieldId("variant")} label="Variant">
                      <Select
                        id={fieldId("variant")}
                        value={current().variant}
                        options={variantOptions()}
                        onChange={(variant) => update({ variant })}
                      />
                    </FormField>
                  </div>
                  <FormField
                    id={fieldId("task")}
                    label="Gradle task"
                    description="Builds the APK Run installs. The module's assemble task by default."
                    error={shownErrors().task}
                  >
                    <Input
                      id={fieldId("task")}
                      size="sm"
                      mono
                      value={current().task}
                      state={shownErrors().task ? "error" : "default"}
                      onInput={(task) => update({ task })}
                    />
                  </FormField>
                  <FormField id={fieldId("target")} label="Target device">
                    <Select
                      id={fieldId("target")}
                      value={current().target}
                      options={targetOptions()}
                      onChange={(target) => update({ target })}
                    />
                  </FormField>
                  <FormField id={fieldId("launch")} label="Launch" error={shownErrors().launch}>
                    <Select
                      id={fieldId("launch")}
                      value={current().launchKind}
                      options={LAUNCH_OPTIONS}
                      onChange={(kind) =>
                        update({ launchKind: kind as RunConfigurationDraft["launchKind"] })
                      }
                    />
                  </FormField>
                  <Show when={current().launchKind === "activity"}>
                    <FormField id={fieldId("activity")} label="Activity">
                      <Input
                        id={fieldId("activity")}
                        size="sm"
                        mono
                        placeholder=".MainActivity"
                        value={current().activity}
                        state={shownErrors().launch ? "error" : "default"}
                        onInput={(activity) => update({ activity })}
                      />
                    </FormField>
                  </Show>
                  <Show when={current().launchKind === "deepLink"}>
                    <FormField id={fieldId("uri")} label="Deep link">
                      <Input
                        id={fieldId("uri")}
                        size="sm"
                        mono
                        placeholder="myapp://home"
                        value={current().uri}
                        state={shownErrors().launch ? "error" : "default"}
                        onInput={(uri) => update({ uri })}
                      />
                    </FormField>
                  </Show>
                  <FormField
                    id={fieldId("logcat")}
                    label="Logcat filter"
                    description="Applied to Logcat after launch. Empty keeps the current filter."
                    error={shownErrors().logcatFilter}
                  >
                    <Input
                      id={fieldId("logcat")}
                      size="sm"
                      mono
                      placeholder="package:mine"
                      value={current().logcatFilter}
                      state={shownErrors().logcatFilter ? "error" : "default"}
                      onInput={(logcatFilter) => update({ logcatFilter })}
                    />
                  </FormField>
                  <section class={styles.plan} aria-labelledby="run-config-plan-title">
                    <h3 id="run-config-plan-title" class={styles.planTitle}>
                      Resolved plan
                    </h3>
                    <Show
                      when={current().savedName !== null}
                      fallback={<p class={styles.note}>Save to see what Run App would do.</p>}
                    >
                      <Show when={dirty()}>
                        <p class={styles.note}>Shows the saved configuration. Save to update it.</p>
                      </Show>
                      <Show when={plan()} fallback={<p class={styles.note}>Resolving…</p>}>
                        {(result) => (
                          <Show
                            when={result().ok}
                            fallback={
                              <Alert
                                variant="warning"
                                action={
                                  <Show when={avdToLaunch()}>
                                    {(avd) => (
                                      <Button
                                        size="xs"
                                        variant="primary"
                                        disabled={deviceState.launchingAvd === avd()}
                                        onClick={() => void launchAvd(avd())}
                                      >
                                        Launch AVD
                                      </Button>
                                    )}
                                  </Show>
                                }
                              >
                                {result().text}
                              </Alert>
                            }
                          >
                            <output class={styles.planText} data-testid="run-config-plan">
                              {result().text}
                            </output>
                          </Show>
                        )}
                      </Show>
                    </Show>
                  </section>
                  <Show when={saveError()}>
                    <Alert variant="error">{saveError()}</Alert>
                  </Show>
                </form>
              )}
            </Show>
          </div>
          <div class={styles.footer}>
            <Button variant="ghost" onClick={() => void close()}>
              Close
            </Button>
            <Button
              variant="primary"
              disabled={!draft() || (!dirty() && draft()?.savedName !== null)}
              loading={saving()}
              onClick={() => void save()}
            >
              Save
            </Button>
          </div>
        </div>
      </div>
    </Portal>
  );
}
