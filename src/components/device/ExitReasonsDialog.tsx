/**
 * ExitReasonsDialog — why the app's processes exited on the selected device,
 * from the exit history Android 11+ keeps (crashes, ANRs, low-memory and
 * system kills), including exits that never reached Logcat.
 */
import { type JSX, For, Match, Show, Switch, createEffect, createSignal, on } from "solid-js";
import { Portal } from "solid-js/web";
import type { AppExitReason, AppExitReasons, AppExitRecord } from "@/bindings";
import { formatError, getExitReasons } from "@/lib/tauri-api";
import { selectedDevice } from "@/stores/device.store";
import {
  Alert,
  Badge,
  type BadgeVariant,
  Button,
  EmptyState,
  Input,
  ScrollArea,
  Spinner,
  modalFocus,
} from "@/components/ui";
import styles from "./ExitReasonsDialog.module.css";

const [open, setOpen] = createSignal(false);

export function openExitReasonsDialog(): void {
  setOpen(true);
}

export function closeExitReasonsDialog(): void {
  setOpen(false);
}

type LoadState =
  | { kind: "noDevice" }
  | { kind: "loading" }
  | { kind: "loaded"; result: AppExitReasons }
  | { kind: "error"; message: string };

const REASON_LABELS: Record<AppExitReason, string> = {
  crash: "Crash",
  crashNative: "Native crash",
  anr: "ANR",
  lowMemory: "Low memory",
  exitSelf: "Exited",
  signaled: "Killed by signal",
  userRequested: "Stopped by user",
  userStopped: "User stopped",
  dependencyDied: "Dependency died",
  excessiveResourceUsage: "Excessive resource use",
  initializationFailure: "Initialization failed",
  permissionChange: "Permission changed",
  freezer: "Freezer",
  other: "Killed by system",
  packageStateChange: "Package state changed",
  packageUpdated: "Package updated",
  unknown: "Unknown",
};

function reasonVariant(reason: AppExitReason): BadgeVariant {
  switch (reason) {
    case "crash":
    case "crashNative":
    case "anr":
    case "initializationFailure":
      return "error";
    case "lowMemory":
    case "excessiveResourceUsage":
    case "freezer":
    case "signaled":
      return "warning";
    default:
      return "default";
  }
}

function isCrash(record: AppExitRecord): boolean {
  return record.reason === "crash" || record.reason === "crashNative" || record.reason === "anr";
}

function formatKb(kb: number): string {
  if (kb >= 1024 * 1024) return `${(kb / (1024 * 1024)).toFixed(1)} GB`;
  if (kb >= 1024) return `${Math.floor(kb / 1024)} MB`;
  return `${kb} KB`;
}

/** Process, pid, importance, and memory, as one line. */
function recordMeta(record: AppExitRecord): string {
  const parts: string[] = [];
  if (record.processName) {
    parts.push(
      record.pid !== null ? `${record.processName} (pid ${record.pid})` : record.processName
    );
  } else if (record.pid !== null) {
    parts.push(`pid ${record.pid}`);
  }
  if (record.subReason && record.subReasonCode !== 0) parts.push(record.subReason);
  if (record.status !== null && record.status !== 0) {
    const signal = record.reason === "signaled" || record.reason === "crashNative";
    parts.push(signal ? `signal ${record.status}` : `status ${record.status}`);
  }
  if (record.importanceName) parts.push(`was ${record.importanceName}`);
  if (record.pssKb !== null && record.rssKb !== null && (record.pssKb > 0 || record.rssKb > 0)) {
    parts.push(`PSS ${formatKb(record.pssKb)}, RSS ${formatKb(record.rssKb)}`);
  }
  return parts.join(" · ");
}

export function ExitReasonsDialog(): JSX.Element {
  const [pkg, setPkg] = createSignal("");
  const [state, setState] = createSignal<LoadState>({ kind: "loading" });
  // Drops a response that arrives after a newer request or after closing.
  let request = 0;

  const onlineDevice = () => {
    const device = selectedDevice();
    return device?.connectionState === "online" ? device : null;
  };

  async function load(): Promise<void> {
    const id = ++request;
    const device = onlineDevice();
    if (!device) {
      setState({ kind: "noDevice" });
      return;
    }
    setState({ kind: "loading" });
    try {
      const result = await getExitReasons(device.serial, pkg().trim() || null);
      if (id === request) setState({ kind: "loaded", result });
    } catch (e) {
      if (id === request) setState({ kind: "error", message: formatError(e) });
    }
  }

  createEffect(
    on([open, () => onlineDevice()?.serial], ([isOpen]) => {
      if (isOpen) void load();
      else request++;
    })
  );

  const loaded = () => {
    const s = state();
    return s.kind === "loaded" ? s.result : null;
  };
  const errorMessage = () => {
    const s = state();
    return s.kind === "error" ? s.message : null;
  };

  return (
    <Show when={open()}>
      <Portal>
        <div class={styles.backdrop} onClick={closeExitReasonsDialog}>
          <div
            ref={(el) => modalFocus(el, { onEscape: closeExitReasonsDialog })}
            class={styles.box}
            role="dialog"
            aria-modal="true"
            aria-labelledby="exit-reasons-title"
            onClick={(e) => e.stopPropagation()}
          >
            <div class={styles.header}>
              <h2 id="exit-reasons-title" class={styles.title}>
                App Exit Reasons
              </h2>
              <Show when={loaded()}>
                {(result) => (
                  <span class={styles.subtitle}>
                    {result().package} on {onlineDevice()?.name ?? result().serial}
                    {result().apiLevel !== null ? ` · API ${result().apiLevel}` : ""}
                  </span>
                )}
              </Show>
            </div>

            <div class={styles.controls}>
              <Input
                class={styles.packageInput}
                size="sm"
                mono
                value={pkg()}
                placeholder="Package (default: the project's app)"
                ariaLabel="Package"
                spellcheck={false}
                onInput={setPkg}
                onKeyDown={(e) => {
                  if (e.key === "Enter") void load();
                }}
              />
              <Button
                variant="outline"
                size="sm"
                loading={state().kind === "loading"}
                onClick={() => void load()}
              >
                Refresh
              </Button>
            </div>

            <ScrollArea class={styles.body}>
              <Switch>
                <Match when={state().kind === "noDevice"}>
                  <EmptyState
                    icon="device"
                    title="No device selected"
                    description="Select an online device in the device sidebar, then refresh."
                    density="compact"
                  />
                </Match>
                <Match when={state().kind === "loading"}>
                  <div class={styles.loading}>
                    <Spinner size="sm" />
                    Reading the exit history…
                  </div>
                </Match>
                <Match when={errorMessage()}>
                  {(message) => (
                    <Alert variant="error" title="Could not read exit reasons">
                      {message()}
                    </Alert>
                  )}
                </Match>
                <Match when={loaded()?.supported === false && loaded()}>
                  {(result) => (
                    <Alert variant="info" title="Not available on this device">
                      {result().message}
                    </Alert>
                  )}
                </Match>
                <Match when={loaded()?.records.length === 0 && loaded()}>
                  {(result) => (
                    <EmptyState
                      icon="check"
                      title="No exits recorded"
                      description={result().message ?? undefined}
                      density="compact"
                    />
                  )}
                </Match>
                <Match when={loaded()}>
                  {(result) => (
                    <>
                      <ol class={styles.list} aria-label="Process exits, newest first">
                        <For each={result().records}>
                          {(record) => (
                            <li class={styles.record}>
                              <div class={styles.recordHead}>
                                <Badge size="xs" variant={reasonVariant(record.reason)}>
                                  {REASON_LABELS[record.reason]}
                                </Badge>
                                <span class={styles.time}>
                                  {record.timestamp ?? "Time unknown"}
                                </span>
                              </div>
                              <Show when={recordMeta(record)}>
                                {(meta) => <div class={styles.meta}>{meta()}</div>}
                              </Show>
                              <Show when={record.description}>
                                {(description) => (
                                  <div class={styles.description}>{description()}</div>
                                )}
                              </Show>
                            </li>
                          )}
                        </For>
                      </ol>
                      <Show when={result().totalRecords > result().records.length}>
                        <p class={styles.note}>
                          Showing the newest {result().records.length} of {result().totalRecords}.
                        </p>
                      </Show>
                      <Show when={result().records.some(isCrash)}>
                        <p class={styles.note}>
                          Stack traces are in Logcat only if it was running when the app crashed.
                        </p>
                      </Show>
                    </>
                  )}
                </Match>
              </Switch>
            </ScrollArea>

            <div class={styles.footer}>
              <Button variant="secondary" size="sm" onClick={closeExitReasonsDialog}>
                Close
              </Button>
            </div>
          </div>
        </div>
      </Portal>
    </Show>
  );
}
