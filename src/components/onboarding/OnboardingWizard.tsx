/**
 * First-run (and re-openable) setup: Android SDK / JDK detection, telemetry consent,
 * and optional workflow toggles. Completes by setting `onboardingCompleted` in settings.
 */

import { type JSX, Show, For, createSignal, createEffect, onMount, onCleanup } from "solid-js";
import { onboardingWizardOpen, closeOnboardingWizard } from "@/stores/onboarding.store";
import { settingsState, updateSetting, setAppSetting } from "@/stores/settings.store";
import { AndroidSdkStatus, JavaStatus } from "@/components/settings/ToolStatus";
import { SettingRow, SettingToggle } from "@/components/settings/SettingRow";
import { healthChecks, refreshHealthChecks } from "@/stores/health.store";
import { showToast } from "@/components/common/Toast";
import Icon from "@/components/common/Icon";
import type { CheckStatus } from "@/stores/health.store";

const STEP_LABELS = ["Welcome", "Environment", "Privacy", "Workflow", "Summary"];

const STATUS_DOT: Record<CheckStatus, string> = {
  ok: "var(--success)",
  warning: "var(--warning)",
  error: "var(--error)",
  loading: "var(--text-muted)",
  skip: "var(--text-disabled)",
};

function StepDots(props: { current: number; total: number }): JSX.Element {
  return (
    <div style={{ display: "flex", gap: "6px", "justify-content": "center", "margin-bottom": "8px" }}>
      <For each={Array.from({ length: props.total }, (_, i) => i)}>
        {(i) => (
          <span
            style={{
              width: "8px",
              height: "8px",
              "border-radius": "50%",
              background: i === props.current ? "var(--accent)" : "var(--border)",
              opacity: i === props.current ? "1" : "0.5",
            }}
          />
        )}
      </For>
    </div>
  );
}

export function OnboardingWizard(): JSX.Element {
  const [step, setStep] = createSignal(0);
  const [telemetryRestartHint, setTelemetryRestartHint] = createSignal(false);

  const isRerun = () => settingsState.onboardingCompleted;

  createEffect(() => {
    if (onboardingWizardOpen()) {
      setStep(0);
      setTelemetryRestartHint(false);
    }
  });

  createEffect(() => {
    if (!onboardingWizardOpen()) return;
    if (step() !== 4) return;
    refreshHealthChecks();
  });

  onMount(() => {
    const onKey = (e: KeyboardEvent) => {
      if (!onboardingWizardOpen()) return;
      if (e.key === "Escape") {
        e.preventDefault();
        if (step() === 0) {
          dismissOnboarding();
        } else {
          setStep((s) => s - 1);
        }
      }
    };
    window.addEventListener("keydown", onKey);
    onCleanup(() => window.removeEventListener("keydown", onKey));
  });

  function dismissOnboarding() {
    setAppSetting("onboardingCompleted", true);
    closeOnboardingWizard();
  }

  function finishOnboarding() {
    setAppSetting("onboardingCompleted", true);
    closeOnboardingWizard();
    if (telemetryRestartHint()) {
      showToast("Crash reporting will be active after you restart the app.", "info");
    }
  }

  function next() {
    if (step() < STEP_LABELS.length - 1) {
      setStep((s) => s + 1);
    } else {
      finishOnboarding();
    }
  }

  function back() {
    if (step() > 0) setStep((s) => s - 1);
  }

  function handleTelemetryChoice(enable: boolean) {
    const prev = settingsState.telemetry.enabled;
    updateSetting("telemetry", "enabled", enable);
    if (enable && !prev) {
      setTelemetryRestartHint(true);
    }
    if (!enable) {
      setTelemetryRestartHint(false);
    }
  }

  return (
    <Show when={onboardingWizardOpen()}>
      <div
        style={{
          position: "fixed",
          inset: "0",
          "z-index": "2100",
          background: "rgba(0,0,0,0.45)",
          display: "flex",
          "align-items": "center",
          "justify-content": "center",
          padding: "24px",
        }}
      >
        <div
          role="dialog"
          aria-modal="true"
          aria-label="Setup wizard"
          style={{
            width: "min(520px, 100%)",
            "max-height": "min(640px, 90vh)",
            background: "var(--bg-secondary)",
            border: "1px solid var(--border)",
            "border-radius": "12px",
            "box-shadow": "0 16px 64px rgba(0,0,0,0.45)",
            display: "flex",
            "flex-direction": "column",
            overflow: "hidden",
          }}
        >
          <div
            style={{
              padding: "16px 20px 12px",
              "border-bottom": "1px solid var(--border)",
              "flex-shrink": "0",
            }}
          >
            <div style={{ display: "flex", "align-items": "center", gap: "10px", "margin-bottom": "8px" }}>
              <div
                style={{
                  width: "40px",
                  height: "40px",
                  background: "var(--accent)",
                  "border-radius": "10px",
                  display: "flex",
                  "align-items": "center",
                  "justify-content": "center",
                }}
              >
                <Icon name="terminal" size={22} color="#fff" />
              </div>
              <div style={{ flex: "1" }}>
                <div style={{ "font-size": "15px", "font-weight": "600", color: "var(--text-primary)" }}>
                  Setup
                </div>
                <div style={{ "font-size": "11px", color: "var(--text-muted)" }}>
                  Step {step() + 1} of {STEP_LABELS.length}: {STEP_LABELS[step()]}
                </div>
              </div>
            </div>
            <StepDots current={step()} total={STEP_LABELS.length} />
            <Show when={isRerun()}>
              <div
                style={{
                  "font-size": "11px",
                  color: "var(--text-muted)",
                  background: "var(--bg-tertiary)",
                  padding: "6px 10px",
                  "border-radius": "6px",
                  "margin-top": "4px",
                }}
              >
                You have completed setup before. Change paths or preferences below, then finish.
              </div>
            </Show>
          </div>

          <div style={{ flex: "1", overflow: "auto", padding: "20px" }}>
            <Show when={step() === 0}>
              <div style={{ display: "flex", "flex-direction": "column", gap: "12px" }}>
                <h2 style={{ margin: "0", "font-size": "17px", color: "var(--text-primary)" }}>
                  Welcome to Android Dev Companion
                </h2>
                <p style={{ margin: "0", "font-size": "13px", color: "var(--text-muted)", "line-height": "1.5" }}>
                  Build logs, logcat, and device management for your Android projects. This short setup finds your Android
                  SDK and JDK (for Gradle and tooling), lets you choose privacy options, and optional workflow
                  defaults.
                </p>
                <p style={{ margin: "0", "font-size": "12px", color: "var(--text-muted)", "line-height": "1.5" }}>
                  Language features for Kotlin use your JDK and project once you open a Gradle project (no separate
                  download step here).
                </p>
              </div>
            </Show>

            <Show when={step() === 1}>
              <div style={{ display: "flex", "flex-direction": "column", gap: "16px" }}>
                <p style={{ margin: "0", "font-size": "12px", color: "var(--text-muted)" }}>
                  Auto-detect uses your shell profile and common install locations (important if you launch from the
                  Dock).
                </p>
                <div>
                  <div style={{ "font-size": "12px", "font-weight": "600", color: "var(--text-primary)", margin: "0 0 8px" }}>
                    Android SDK
                  </div>
                  <AndroidSdkStatus />
                </div>
                <div>
                  <div style={{ "font-size": "12px", "font-weight": "600", color: "var(--text-primary)", margin: "0 0 8px" }}>
                    Java JDK (JAVA_HOME)
                  </div>
                  <JavaStatus />
                </div>
              </div>
            </Show>

            <Show when={step() === 2}>
              <div style={{ display: "flex", "flex-direction": "column", gap: "14px" }}>
                <p style={{ margin: "0", "font-size": "13px", color: "var(--text-primary)", "line-height": "1.5" }}>
                  Optional anonymous crash reports help fix bugs. Only app-side diagnostics are sent — not your project
                  paths, source, or logs. You can change this anytime in Settings.
                </p>
                <div style={{ display: "flex", "flex-direction": "column", gap: "8px" }}>
                  <button
                    type="button"
                    onClick={() => handleTelemetryChoice(true)}
                    style={{
                      padding: "10px 14px",
                      "border-radius": "8px",
                      border:
                        settingsState.telemetry.enabled
                          ? "2px solid var(--accent)"
                          : "1px solid var(--border)",
                      background: settingsState.telemetry.enabled ? "var(--accent-bg)" : "var(--bg-primary)",
                      color: "var(--text-primary)",
                      cursor: "pointer",
                      "text-align": "left",
                      "font-size": "13px",
                    }}
                  >
                    <strong>Enable crash reporting</strong>
                    <div style={{ "font-size": "11px", color: "var(--text-muted)", "margin-top": "4px" }}>
                      Sends anonymous crash reports to improve stability.
                    </div>
                  </button>
                  <button
                    type="button"
                    onClick={() => handleTelemetryChoice(false)}
                    style={{
                      padding: "10px 14px",
                      "border-radius": "8px",
                      border:
                        !settingsState.telemetry.enabled
                          ? "2px solid var(--accent)"
                          : "1px solid var(--border)",
                      background: !settingsState.telemetry.enabled ? "var(--accent-bg)" : "var(--bg-primary)",
                      color: "var(--text-primary)",
                      cursor: "pointer",
                      "text-align": "left",
                      "font-size": "13px",
                    }}
                  >
                    <strong>Do not send crash reports</strong>
                    <div style={{ "font-size": "11px", color: "var(--text-muted)", "margin-top": "4px" }}>
                      Default — no crash data is sent.
                    </div>
                  </button>
                </div>
              </div>
            </Show>

            <Show when={step() === 3}>
              <div style={{ display: "flex", "flex-direction": "column", gap: "4px" }}>
                <p style={{ margin: "0 0 8px", "font-size": "12px", color: "var(--text-muted)" }}>
                  Optional defaults — you can change these later in Settings.
                </p>
                <SettingRow
                  label="Start MCP server when the app opens"
                  description="For Claude Code integration via Model Context Protocol. Uses a background process."
                >
                  <SettingToggle
                    checked={settingsState.mcp.autoStart}
                    onChange={(v) => updateSetting("mcp", "autoStart", v)}
                  />
                </SettingRow>
                <SettingRow
                  label="Auto-start logcat when a device connects"
                  description="Streams device logs automatically when you plug in a device or start an emulator."
                >
                  <SettingToggle
                    checked={settingsState.logcat.autoStart}
                    onChange={(v) => updateSetting("logcat", "autoStart", v)}
                  />
                </SettingRow>
              </div>
            </Show>

            <Show when={step() === 4}>
              <div style={{ display: "flex", "flex-direction": "column", gap: "10px" }}>
                <p style={{ margin: "0", "font-size": "12px", color: "var(--text-muted)" }}>
                  Quick environment check (same as Health Center). Fix issues in Settings anytime.
                </p>
                <For each={healthChecks()}>
                  {(c) => (
                    <div
                      style={{
                        display: "flex",
                        "align-items": "flex-start",
                        gap: "10px",
                        padding: "8px 10px",
                        background: "var(--bg-primary)",
                        "border-radius": "6px",
                        "font-size": "12px",
                      }}
                    >
                      <span
                        style={{
                          width: "8px",
                          height: "8px",
                          "border-radius": "50%",
                          background: STATUS_DOT[c.status],
                          "margin-top": "4px",
                          "flex-shrink": "0",
                        }}
                      />
                      <div style={{ flex: "1", "min-width": "0" }}>
                        <div style={{ color: "var(--text-primary)", "font-weight": "500" }}>{c.name}</div>
                        <div style={{ color: "var(--text-muted)", "font-size": "11px", "margin-top": "2px" }}>
                          {c.detail}
                        </div>
                      </div>
                    </div>
                  )}
                </For>
              </div>
            </Show>
          </div>

          <div
            style={{
              padding: "12px 20px",
              "border-top": "1px solid var(--border)",
              display: "flex",
              "justify-content": "space-between",
              "align-items": "center",
              "flex-shrink": "0",
              gap: "8px",
            }}
          >
            <div style={{ display: "flex", gap: "8px" }}>
              <Show when={step() === 0}>
                <button
                  type="button"
                  onClick={dismissOnboarding}
                  style={{
                    padding: "6px 12px",
                    "font-size": "12px",
                    background: "none",
                    border: "1px solid var(--border)",
                    color: "var(--text-muted)",
                    "border-radius": "6px",
                    cursor: "pointer",
                  }}
                >
                  Skip setup
                </button>
              </Show>
              <Show when={step() > 0}>
                <button
                  type="button"
                  onClick={back}
                  style={{
                    padding: "6px 12px",
                    "font-size": "12px",
                    background: "none",
                    border: "1px solid var(--border)",
                    color: "var(--text-primary)",
                    "border-radius": "6px",
                    cursor: "pointer",
                  }}
                >
                  Back
                </button>
              </Show>
            </div>
            <button
              type="button"
              onClick={next}
              style={{
                padding: "8px 18px",
                "font-size": "12px",
                "font-weight": "500",
                background: "var(--accent)",
                border: "none",
                color: "#fff",
                "border-radius": "6px",
                cursor: "pointer",
              }}
            >
              {step() === STEP_LABELS.length - 1 ? "Finish" : "Continue"}
            </button>
          </div>
        </div>
      </div>
    </Show>
  );
}
