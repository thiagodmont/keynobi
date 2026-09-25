/**
 * Browser (Solid / WebView) crash reporting — optional, opt-in, allowlist-only.
 *
 * - DSN: `import.meta.env.VITE_SENTRY_DSN` at build time only — never commit real values.
 * - Tauri **CSP** (`tauri.conf.json` `connect-src`) must allow Sentry ingest HTTPS hosts.
 * - The SDK runs only while `settings.telemetry.enabled` is true and a DSN is set. Start/stop
 *   requests are serialized; the last requested state wins.
 * - Every outgoing event is rebuilt from an allowlist: error type, release, environment,
 *   app-bundle stack frames (function, bundle file, line, column), source-map debug IDs, and the
 *   IPC error kind when the error is an `AppError`. Messages, exception values, breadcrumbs, tags,
 *   extra, user, request and contexts are never copied.
 * - The transport re-checks consent and forwards only event items, so opting out stops uploads
 *   immediately and nothing else (sessions, client reports) is ever sent.
 */

import * as Sentry from "@sentry/browser";
import type { BrowserOptions, ErrorEvent, EventHint, Exception, StackFrame } from "@sentry/browser";
import type { AppError } from "@/bindings";
import { settingsState } from "@/stores/settings.store";

type TransportFactory = NonNullable<BrowserOptions["transport"]>;
type Transport = ReturnType<TransportFactory>;
type Envelope = Parameters<Transport["send"]>[0];
type Mechanism = NonNullable<Exception["mechanism"]>;
type DebugImage = NonNullable<NonNullable<ErrorEvent["debug_meta"]>["images"]>[number];

/** Known error codes: the `kind` of an IPC `AppError`. */
const ERROR_CODES: Record<AppError["kind"], true> = {
  notFound: true,
  permissionDenied: true,
  invalidInput: true,
  io: true,
  processFailed: true,
  settingsError: true,
  mcpError: true,
  other: true,
};

/** Most frames kept per exception (the newest ones, nearest the error). */
const MAX_FRAMES = 100;
/** Most exceptions kept per event (linked causes). */
const MAX_EXCEPTIONS = 10;
const CLOSE_TIMEOUT_MS = 2000;

const ERROR_TYPE = /^[A-Za-z_$][\w$]{0,63}$/;
const FUNCTION_NAME = /^[\w$.<>[\]? -]{1,200}$/;
const MECHANISM_TYPE = /^(?:generic|chained|auto(?:\.[a-z_]+)+)$/;
const DEBUG_ID = /^[0-9a-f]{8}-?[0-9a-f]{4}-?[0-9a-f]{4}-?[0-9a-f]{4}-?[0-9a-f]{12}$/i;
/** Origins the app bundle is served from (production, dev server, and our own rewritten form). */
const APP_URL =
  /^(?:app:\/\/|tauri:\/\/localhost|https?:\/\/(?:tauri\.)?localhost(?::\d+)?|https?:\/\/127\.0\.0\.1(?::\d+)?)(\/[^?#]*)/;
const BUNDLE_ROOTS = new Set(["assets", "src", "node_modules"]);
const DIR_SEGMENT = /^(?:@?[\w-]+|\.vite)$/;
const FILE_SEGMENT = /^[\w.-]+\.(?:js|mjs|cjs|jsx|ts|tsx)$/;

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

/** The `kind` of an `AppError`-shaped value, when it is a known one. */
export function errorCodeOf(value: unknown): string | undefined {
  if (!value || typeof value !== "object") return undefined;
  const kind = (value as { kind?: unknown }).kind;
  return typeof kind === "string" && Object.prototype.hasOwnProperty.call(ERROR_CODES, kind)
    ? kind
    : undefined;
}

/**
 * Reduces a frame URL to `app:///<path>` when it points into the app bundle; anything else
 * (file paths, other origins, dev-server absolute paths, query strings) yields `undefined`.
 */
export function appBundleUrl(url: string | undefined): string | undefined {
  const match = url ? APP_URL.exec(url) : null;
  const path = match?.[1];
  if (!path || path.length > 256) return undefined;
  const segments = path.slice(1).split("/");
  const file = segments.pop();
  if (!file || !FILE_SEGMENT.test(file)) return undefined;
  if (!BUNDLE_ROOTS.has(segments[0] ?? "")) return undefined;
  if (!segments.every((s) => DIR_SEGMENT.test(s))) return undefined;
  return `app://${path}`;
}

function allowlistFrame(frame: StackFrame): StackFrame | undefined {
  const url = appBundleUrl(frame.abs_path) ?? appBundleUrl(frame.filename);
  if (!url) return undefined;
  const out: StackFrame = { filename: url, abs_path: url, in_app: true };
  if (frame.function && FUNCTION_NAME.test(frame.function)) out.function = frame.function;
  if (typeof frame.lineno === "number") out.lineno = frame.lineno;
  if (typeof frame.colno === "number") out.colno = frame.colno;
  return out;
}

function allowlistMechanism(mechanism: Mechanism | undefined): Mechanism | undefined {
  if (!mechanism || !MECHANISM_TYPE.test(mechanism.type)) return undefined;
  const out: Mechanism = { type: mechanism.type };
  if (typeof mechanism.handled === "boolean") out.handled = mechanism.handled;
  if (typeof mechanism.exception_id === "number") out.exception_id = mechanism.exception_id;
  if (typeof mechanism.parent_id === "number") out.parent_id = mechanism.parent_id;
  return out;
}

function allowlistException(ex: Exception): Exception {
  const out: Exception = {
    type: ex.type && ERROR_TYPE.test(ex.type) ? ex.type : "Error",
  };
  const mechanism = allowlistMechanism(ex.mechanism);
  if (mechanism) out.mechanism = mechanism;
  const frames = (ex.stacktrace?.frames ?? [])
    .map(allowlistFrame)
    .filter((f): f is StackFrame => f !== undefined)
    .slice(-MAX_FRAMES);
  if (frames.length > 0) out.stacktrace = { frames };
  return out;
}

function allowlistDebugImages(event: ErrorEvent): DebugImage[] {
  const images: DebugImage[] = [];
  for (const image of event.debug_meta?.images ?? []) {
    if (image.type !== "sourcemap") continue;
    const codeFile = appBundleUrl(image.code_file);
    if (codeFile && DEBUG_ID.test(image.debug_id)) {
      images.push({ type: "sourcemap", code_file: codeFile, debug_id: image.debug_id });
    }
  }
  return images;
}

function safeString(value: unknown, pattern: RegExp): string | undefined {
  return typeof value === "string" && pattern.test(value) ? value : undefined;
}

/**
 * Builds the event that is actually sent, copying only allowlisted fields. Returns `null` for
 * events without an exception (plain messages are never sent).
 */
export function buildAllowlistedEvent(event: ErrorEvent, errorCode?: string): ErrorEvent | null {
  const values = (event.exception?.values ?? []).slice(-MAX_EXCEPTIONS).map(allowlistException);
  if (values.length === 0) return null;

  const out: ErrorEvent = {
    type: undefined,
    platform: "javascript",
    exception: { values },
  };
  const eventId = safeString(event.event_id, /^[0-9a-f]{32}$/);
  if (eventId) out.event_id = eventId;
  if (typeof event.timestamp === "number") out.timestamp = event.timestamp;
  if (event.level) out.level = event.level;
  const release = safeString(event.release, /^[\w.+-]{1,64}$/);
  if (release) out.release = release;
  const environment = safeString(event.environment, /^[a-z]{1,32}$/);
  if (environment) out.environment = environment;

  const code = errorCodeOf({ kind: errorCode ?? event.tags?.error_code });
  if (code) out.tags = { error_code: code };

  const images = allowlistDebugImages(event);
  if (images.length > 0) out.debug_meta = { images };
  return out;
}

/** Keeps only allowlisted event items; the envelope header keeps only its id and send time. */
export function allowlistEnvelope(envelope: Envelope): Envelope | null {
  const [headers, items] = envelope;
  const kept: [{ type: "event" }, ErrorEvent][] = [];
  for (const [itemHeaders, payload] of items) {
    if (itemHeaders.type !== "event") continue;
    const event = buildAllowlistedEvent(payload as ErrorEvent);
    if (event) kept.push([{ type: "event" }, event]);
  }
  if (kept.length === 0) return null;
  const outHeaders = { event_id: headers.event_id, sent_at: headers.sent_at };
  return [outHeaders, kept] as unknown as Envelope;
}

function consentGatedTransport(isEnabled: () => boolean, base: TransportFactory): TransportFactory {
  return (options) => {
    const inner = base(options);
    return {
      send(envelope) {
        if (!isEnabled()) return Promise.resolve({});
        const allowed = allowlistEnvelope(envelope);
        return allowed ? inner.send(allowed) : Promise.resolve({});
      },
      flush: (timeout) => inner.flush(timeout),
    };
  };
}

export interface SentryWebDeps {
  readDsn: () => string | undefined;
  isEnabled: () => boolean;
  transport: TransportFactory;
}

export interface SentryWebController {
  /** Start or stop the SDK to match the current settings. Calls run in order. */
  sync(): Promise<void>;
  /** Report an error when crash reporting is on. */
  capture(error: unknown): void;
  isActive(): boolean;
}

export function createSentryWebController(deps: SentryWebDeps): SentryWebController {
  let active = false;
  let queue: Promise<void> = Promise.resolve();

  function beforeSend(event: ErrorEvent, hint: EventHint): ErrorEvent | null {
    if (!deps.isEnabled()) return null;
    return buildAllowlistedEvent(event, errorCodeOf(hint.originalException));
  }

  async function apply(): Promise<void> {
    const dsn = deps.readDsn();
    const wanted = Boolean(dsn && deps.isEnabled());
    if (wanted && !active && dsn) {
      Sentry.init({
        dsn,
        debug: import.meta.env.DEV && import.meta.env.MODE !== "test",
        environment: import.meta.env.MODE === "production" ? "production" : "development",
        release: import.meta.env.VITE_APP_VERSION,
        sendDefaultPii: false,
        // Off so plain messages never gain a synthetic exception and slip past the allowlist.
        attachStacktrace: false,
        // Only integrations whose output passes through the allowlist.
        defaultIntegrations: false,
        integrations: [
          Sentry.globalHandlersIntegration(),
          Sentry.linkedErrorsIntegration(),
          Sentry.dedupeIntegration(),
        ],
        maxBreadcrumbs: 0,
        sendClientReports: false,
        beforeBreadcrumb: () => null,
        beforeSend,
        beforeSendTransaction: () => null,
        transport: consentGatedTransport(deps.isEnabled, deps.transport),
      });
      active = true;
    } else if (!wanted && active) {
      active = false;
      await Sentry.close(CLOSE_TIMEOUT_MS);
    }
  }

  return {
    sync() {
      queue = queue.then(apply).catch((err: unknown) => {
        console.error("[telemetry] Failed to update crash reporting:", err);
      });
      return queue;
    },
    capture(error) {
      if (active && deps.isEnabled()) Sentry.captureException(error);
    },
    isActive: () => active,
  };
}

const controller = createSentryWebController({
  readDsn,
  isEnabled: isTelemetryEnabled,
  transport: Sentry.makeFetchTransport,
});

/**
 * Keeps the browser Sentry client in sync with `settings.telemetry.enabled` and `VITE_SENTRY_DSN`.
 * Call from a reactive effect whenever telemetry may change, and after `loadSettings()`.
 */
export function initSentryWeb(): void {
  if (import.meta.env.MODE === "test") {
    return;
  }
  void controller.sync();
}

/** Report a rendering / boundary error when telemetry is on. */
export function captureSentryException(error: unknown): void {
  controller.capture(error);
}
