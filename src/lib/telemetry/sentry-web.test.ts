import { afterEach, describe, expect, it } from "vitest";
import * as Sentry from "@sentry/browser";
import type { BrowserOptions, ErrorEvent } from "@sentry/browser";
import {
  appBundleUrl,
  buildAllowlistedEvent,
  createSentryWebController,
  errorCodeOf,
  type SentryWebController,
} from "./sentry-web";

type TransportFactory = NonNullable<BrowserOptions["transport"]>;
type Envelope = Parameters<ReturnType<TransportFactory>["send"]>[0];

/** Synthetic secrets that must never appear in anything sent. */
const SECRETS = [
  "alice",
  "/Users/",
  "/opt/secret-project",
  "com.secret.app",
  "R58M123ABC",
  "emulator-5554",
  "abc123SECRET",
  "api.example.com",
  "alice@example.com",
  "AndroidRuntime",
  "FATAL EXCEPTION",
];

const HOME_PATH = "/Users/alice/Projects/com.secret.app/app/src/main/Main.kt";
const OTHER_PATH = "/opt/secret-project/build/output.apk";
const URL_WITH_TOKEN = "https://api.example.com/v1/devices?token=abc123SECRET";
const LOGCAT =
  "09-25 12:00:00.000  1234  1234 E AndroidRuntime: FATAL EXCEPTION: main com.secret.app";
const EMAIL = "alice@example.com";
const SECRET_BLOB = `${HOME_PATH} ${OTHER_PATH} ${URL_WITH_TOKEN} ${LOGCAT} ${EMAIL} R58M123ABC emulator-5554`;
const DEBUG_ID = "0a1b2c3d-4e5f-4a6b-8c7d-9e0f1a2b3c4d";

function expectNoSecrets(value: unknown): void {
  const serialized = JSON.stringify(value);
  for (const secret of SECRETS) {
    expect(serialized, `leaked ${secret}`).not.toContain(secret);
  }
}

/** An event carrying secrets in every field the SDK or app could populate. */
function eventWithSecrets(): ErrorEvent {
  return {
    type: undefined,
    event_id: "0123456789abcdef0123456789abcdef",
    timestamp: 1_700_000_000,
    level: "error",
    platform: "javascript",
    release: "0.1.29",
    environment: "production",
    message: SECRET_BLOB,
    logentry: { message: SECRET_BLOB, params: [EMAIL] },
    transaction: URL_WITH_TOKEN,
    server_name: "alice-macbook",
    user: { email: EMAIL, username: "alice", ip_address: "{{auto}}" },
    request: { url: URL_WITH_TOKEN, headers: { Authorization: "Bearer abc123SECRET" } },
    tags: { serial: "R58M123ABC", package: "com.secret.app", error_code: "com.secret.app" },
    extra: { __serialized__: { kind: "io", message: SECRET_BLOB } },
    breadcrumbs: [{ message: LOGCAT, category: "console" }],
    contexts: { project: { root: HOME_PATH }, device: { name: "R58M123ABC" } },
    fingerprint: [SECRET_BLOB],
    exception: {
      values: [
        {
          type: "Error",
          value: `Failed to open ${SECRET_BLOB}`,
          module: "com.secret.app",
          mechanism: {
            type: "auto.browser.global_handlers.onunhandledrejection",
            handled: false,
            data: { path: OTHER_PATH },
          },
          stacktrace: {
            frames: [
              {
                filename: "file:///Users/alice/app/dist/assets/index.js",
                function: "loadProject",
                lineno: 1,
              },
              {
                filename: "https://api.example.com/v1/sdk.js?token=abc123SECRET",
                function: "remote",
                lineno: 2,
              },
              {
                filename: "http://localhost:1420/@fs/Users/alice/node_modules/solid-js/web.js",
                function: "render",
                lineno: 3,
              },
              {
                filename: "tauri://localhost/assets/index-a1b2c3.js?token=abc123SECRET#frag",
                abs_path: "tauri://localhost/assets/index-a1b2c3.js?token=abc123SECRET#frag",
                function: "openProject",
                lineno: 10,
                colno: 20,
                pre_context: [LOGCAT],
                context_line: `const serial = "R58M123ABC";`,
                post_context: [EMAIL],
                vars: { path: HOME_PATH },
              },
            ],
          },
        },
      ],
    },
    debug_meta: {
      images: [
        {
          type: "sourcemap",
          code_file: "tauri://localhost/assets/index-a1b2c3.js?token=abc123SECRET#frag",
          debug_id: DEBUG_ID,
        },
        { type: "sourcemap", code_file: `file://${HOME_PATH}`, debug_id: DEBUG_ID },
      ],
    },
  } as ErrorEvent;
}

describe("appBundleUrl", () => {
  it("rewrites app bundle URLs and drops query strings", () => {
    expect(appBundleUrl("tauri://localhost/assets/index-a1.js?x=1#y")).toBe(
      "app:///assets/index-a1.js"
    );
    expect(appBundleUrl("http://localhost:1420/src/App.tsx?t=123")).toBe("app:///src/App.tsx");
    expect(appBundleUrl("app:///assets/index-a1.js")).toBe("app:///assets/index-a1.js");
  });

  it("rejects file paths, other origins and dev-server absolute paths", () => {
    expect(appBundleUrl(`file://${HOME_PATH}`)).toBeUndefined();
    expect(appBundleUrl("/Users/alice/app/index.js")).toBeUndefined();
    expect(appBundleUrl(URL_WITH_TOKEN)).toBeUndefined();
    expect(appBundleUrl("http://localhost:1420/@fs/Users/alice/x.js")).toBeUndefined();
    expect(appBundleUrl("tauri://localhost/assets/com.secret.app/x.js")).toBeUndefined();
  });
});

describe("errorCodeOf", () => {
  it("returns the kind of a known IPC error only", () => {
    expect(errorCodeOf({ kind: "io", message: HOME_PATH })).toBe("io");
    expect(errorCodeOf({ kind: "com.secret.app" })).toBeUndefined();
    expect(errorCodeOf({ kind: "toString" })).toBeUndefined();
    expect(errorCodeOf(`io: ${HOME_PATH}`)).toBeUndefined();
  });
});

describe("buildAllowlistedEvent", () => {
  it("contains no secrets", () => {
    expectNoSecrets(buildAllowlistedEvent(eventWithSecrets(), "io"));
  });

  it("keeps only the allowlisted fields", () => {
    const out = buildAllowlistedEvent(eventWithSecrets(), "io");
    expect(out).toEqual({
      type: undefined,
      platform: "javascript",
      event_id: "0123456789abcdef0123456789abcdef",
      timestamp: 1_700_000_000,
      level: "error",
      release: "0.1.29",
      environment: "production",
      tags: { error_code: "io" },
      exception: {
        values: [
          {
            type: "Error",
            mechanism: {
              type: "auto.browser.global_handlers.onunhandledrejection",
              handled: false,
            },
            stacktrace: {
              frames: [
                {
                  filename: "app:///assets/index-a1b2c3.js",
                  abs_path: "app:///assets/index-a1b2c3.js",
                  in_app: true,
                  function: "openProject",
                  lineno: 10,
                  colno: 20,
                },
              ],
            },
          },
        ],
      },
      debug_meta: {
        images: [
          { type: "sourcemap", code_file: "app:///assets/index-a1b2c3.js", debug_id: DEBUG_ID },
        ],
      },
    });
  });

  it("drops events without an exception", () => {
    expect(buildAllowlistedEvent({ type: undefined, message: SECRET_BLOB })).toBeNull();
  });

  it("replaces unsafe error types and drops unknown error codes", () => {
    const event = eventWithSecrets();
    event.exception!.values![0]!.type = "com.secret.app";
    const out = buildAllowlistedEvent(event);
    expect(out?.exception?.values?.[0]?.type).toBe("Error");
    expect(out?.tags).toBeUndefined();
  });
});

describe("createSentryWebController", () => {
  const DSN = "https://public@sentry.invalid/1";
  let controller: SentryWebController | undefined;
  let enabled = false;

  function setup(): Envelope[] {
    const sent: Envelope[] = [];
    enabled = false;
    controller = createSentryWebController({
      readDsn: () => DSN,
      isEnabled: () => enabled,
      transport: () => ({
        send: (envelope) => {
          sent.push(envelope);
          return Promise.resolve({});
        },
        flush: () => Promise.resolve(true),
      }),
    });
    return sent;
  }

  async function setEnabled(value: boolean): Promise<void> {
    enabled = value;
    await controller!.sync();
  }

  afterEach(async () => {
    enabled = false;
    await controller?.sync();
    controller = undefined;
  });

  it("sends only allowlisted data for an unhandled IPC rejection", async () => {
    const sent = setup();
    await setEnabled(true);

    const onRejection = (globalThis as { onunhandledrejection?: unknown }).onunhandledrejection;
    expect(typeof onRejection).toBe("function");
    (onRejection as (e: unknown) => void)({
      reason: { kind: "io", message: `Failed to read ${SECRET_BLOB}` },
    });
    (onRejection as (e: unknown) => void)({ reason: `Failed to open ${SECRET_BLOB}` });
    Sentry.addBreadcrumb({ message: LOGCAT });
    Sentry.setTag("serial", "R58M123ABC");
    Sentry.setUser({ email: EMAIL });
    Sentry.captureException(new Error(SECRET_BLOB));
    Sentry.captureMessage(SECRET_BLOB);
    await Sentry.flush(2000);

    expect(sent).toHaveLength(3);
    expectNoSecrets(sent);
    const events = sent.map((envelope) => envelope[1][0]![1] as ErrorEvent);
    expect(events.filter((e) => e.tags?.error_code === "io")).toHaveLength(1);
    expect(events.some((e) => e.exception?.values?.[0]?.type === "UnhandledRejection")).toBe(true);
    for (const envelope of sent) {
      expect(Object.keys(envelope[0]).sort()).toEqual(["event_id", "sent_at"]);
    }
  });

  it("does not send after opt-out, even before the SDK is closed", async () => {
    const sent = setup();
    await setEnabled(true);

    enabled = false; // settings changed; the close has not run yet
    Sentry.captureException(new Error("boom"));
    controller!.capture(new Error("boom"));
    await Sentry.flush(2000);
    expect(sent).toHaveLength(0);

    await controller!.sync();
    expect(controller!.isActive()).toBe(false);
  });

  it("re-enables after off, on, off, on", async () => {
    const sent = setup();
    for (const value of [true, false, true, false, true]) {
      await setEnabled(value);
    }
    expect(controller!.isActive()).toBe(true);
    controller!.capture(new Error("boom"));
    await Sentry.flush(2000);
    expect(sent).toHaveLength(1);
  });

  it("ends in the last requested state when toggled rapidly", async () => {
    const sent = setup();
    await setEnabled(true);

    const pending: Promise<void>[] = [];
    for (const value of [false, true, false, true, false]) {
      enabled = value;
      pending.push(controller!.sync());
    }
    await Promise.all(pending);
    expect(controller!.isActive()).toBe(false);
    Sentry.captureException(new Error("boom"));
    await Sentry.flush(2000);
    expect(sent).toHaveLength(0);

    for (const value of [true, false, true]) {
      enabled = value;
      pending.push(controller!.sync());
    }
    await Promise.all(pending);
    expect(controller!.isActive()).toBe(true);
    controller!.capture(new Error("boom"));
    await Sentry.flush(2000);
    expect(sent).toHaveLength(1);
  });
});
