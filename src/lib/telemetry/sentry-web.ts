/**
 * Browser (Solid / WebView) Sentry integration — optional, privacy-first.
 *
 * - DSN: `import.meta.env.VITE_SENTRY_DSN` at build time only — never commit real values.
 * - Tauri **CSP** (`tauri.conf.json` `connect-src`) must allow Sentry ingest HTTPS hosts or uploads are blocked.
 * - Runtime gate: the SDK is **only** `Sentry.init`'d when `settings.telemetry.enabled` is true (and DSN is set);
 *   when the user opts out, `Sentry.close()` runs so the client is not left active. `beforeSend` still filters if needed.
 * - In Vite dev (`import.meta.env.DEV`), SDK `debug` is on — verbose Sentry logs in the WebView console (off in tests and production).
 * - No default PII, no breadcrumbs, stack filenames stripped of local `file://` / home-like paths.
 *
 * Uses `@sentry/solid` peer router deps are not required; we use `@sentry/browser` only.
 */

import * as Sentry from "@sentry/browser";
import type { ErrorEvent } from "@sentry/core";
import { settingsState } from "@/stores/settings.store";

function isTelemetryEnabled(): boolean {
  return settingsState.telemetry.enabled === true;
}

/** Whether Anonymous crash reporting is on (reads live settings, not a cached flag). */
export function getSentryTelemetryOptIn(): boolean {
  return isTelemetryEnabled();
}

function readDsn(): string | undefined {
  const raw = import.meta.env.VITE_SENTRY_DSN;
  if (typeof raw !== "string" || raw.trim().length === 0) {
    return undefined;
  }
  return raw.trim();
}

/** Exported for tests — redacts path-like segments in frame filenames. */
export function scrubWebFrameFilename(filename: string | undefined): string | undefined {
  if (!filename) {
    return filename;
  }
  const lower = filename.toLowerCase();
  if (
    lower.startsWith("file://") ||
    lower.includes("/users/") ||
    lower.includes("/home/") ||
    lower.includes("\\users\\")
  ) {
    const parts = filename.replace(/\\/g, "/").split("/");
    const base = parts[parts.length - 1];
    return base && base.length > 0 ? base : "<redacted>";
  }
  return filename;
}

/** Exported for tests — minimal in-place scrub of a Sentry error event. */
export function scrubBrowserEvent(event: ErrorEvent): ErrorEvent {
  event.user = undefined;
  event.request = undefined;
  event.server_name = undefined;
  event.breadcrumbs = [];
  if (event.extra) {
    event.extra = {};
  }

  const exceptions = event.exception?.values;
  if (exceptions) {
    for (const ex of exceptions) {
      if (ex.stacktrace?.frames) {
        for (const frame of ex.stacktrace.frames) {
          if (frame.filename) {
            frame.filename = scrubWebFrameFilename(frame.filename);
          }
          frame.abs_path = undefined;
        }
      }
    }
  }

  if (event.contexts?.app) {
    delete event.contexts.app;
  }

  event.tags = {
    ...event.tags,
    "app.layer": "web",
    "build.profile": import.meta.env.DEV ? "debug" : "release",
  };

  return event;
}

function beforeSend(event: ErrorEvent): ErrorEvent | null {
  if (!isTelemetryEnabled()) {
    return null;
  }
  return scrubBrowserEvent(event);
}

function beforeBreadcrumb(): null {
  return null;
}

/**
 * Keeps the browser Sentry client in sync with `settings.telemetry.enabled` and `VITE_SENTRY_DSN`.
 * Call from a reactive effect whenever telemetry may change, and after `loadSettings()`.
 * Does not initialize when telemetry is off — no SDK hooks or transport until the user opts in.
 */
export function initSentryWeb(): void {
  void syncSentryWebWithTelemetry();
}

async function syncSentryWebWithTelemetry(): Promise<void> {
  if (import.meta.env.MODE === "test") {
    return;
  }

  const dsn = readDsn();
  const shouldRun = Boolean(dsn && isTelemetryEnabled());

  if (!shouldRun) {
    if (Sentry.isInitialized()) {
      await Sentry.close(2000);
    }
    return;
  }

  if (Sentry.isInitialized()) {
    return;
  }
  if (!dsn) {
    return;
  }

  Sentry.init({
    dsn,
    debug: import.meta.env.DEV,
    environment: import.meta.env.MODE === "production" ? "production" : "development",
    release: import.meta.env.VITE_APP_VERSION,
    sendDefaultPii: false,
    attachStacktrace: true,
    tracesSampleRate: 1,
    replaysSessionSampleRate: 0,
    replaysOnErrorSampleRate: 0,
    beforeSend,
    beforeBreadcrumb,
  });
}

/** Report a rendering / boundary error when telemetry is allowed and Sentry is initialized. */
export function captureSentryException(error: unknown): void {
  if (!isTelemetryEnabled() || !readDsn() || !Sentry.isInitialized()) {
    return;
  }
  Sentry.captureException(error);
}
