import { type JSX, Show, For, createSignal, onMount, createMemo } from "solid-js";
import { Portal } from "solid-js/web";
import type { AvdInfo } from "@/bindings";
import {
  listSystemImages,
  listDeviceDefinitions,
  createAvdDevice,
  formatError,
} from "@/lib/tauri-api";
import {
  deviceState,
  setSystemImages,
  setDeviceDefinitions,
} from "@/stores/device.store";
import Icon from "@/components/common/Icon";

export interface CreateDeviceDialogProps {
  onClose: () => void;
  onCreated: (avds: AvdInfo[]) => void;
}

export function CreateDeviceDialog(props: CreateDeviceDialogProps): JSX.Element {
  const [loading, setLoading] = createSignal(true);
  const [creating, setCreating] = createSignal(false);
  const [error, setError] = createSignal<string | null>(null);

  const [selectedImageIdx, setSelectedImageIdx] = createSignal(0);
  const [selectedDeviceId, setSelectedDeviceId] = createSignal<string>("");
  const [name, setName] = createSignal("");
  const [nameManuallyEdited, setNameManuallyEdited] = createSignal(false);

  const images = () => deviceState.systemImages;
  const deviceDefs = () => deviceState.deviceDefinitions;

  const selectedImage = createMemo(() => images()[selectedImageIdx()] ?? null);
  const selectedDeviceDef = createMemo(
    () => deviceDefs().find((d) => d.id === selectedDeviceId()) ?? null
  );

  // Auto-suggest AVD name from selected device + API level.
  const suggestedName = createMemo(() => {
    const dev = selectedDeviceDef();
    const img = selectedImage();
    if (!dev || !img) return "";
    const base = dev.name.replace(/\s+/g, "_").replace(/[^a-zA-Z0-9_.-]/g, "");
    return `${base}_API_${img.apiLevel}`;
  });

  const nameToUse = createMemo(() =>
    nameManuallyEdited() ? name() : suggestedName()
  );

  const nameError = createMemo(() => {
    const n = nameToUse();
    if (!n) return null;
    if (/\s/.test(n)) return "Name cannot contain spaces";
    if (!/^[a-zA-Z0-9_.\- ]+$/.test(n)) return "Only letters, numbers, underscores, dashes, and dots";
    if (deviceState.avds.some((a) => a.name === n)) return "An AVD with this name already exists";
    return null;
  });

  onMount(async () => {
    try {
      // Only load if not already cached.
      const [imgs, defs] = await Promise.all([
        images().length > 0 ? Promise.resolve(images()) : listSystemImages(),
        deviceDefs().length > 0 ? Promise.resolve(deviceDefs()) : listDeviceDefinitions(),
      ]);
      setSystemImages(imgs);
      setDeviceDefinitions(defs);

      // Pick sensible defaults.
      if (defs.length > 0) {
        // Prefer a Pixel device.
        const pixel = defs.find((d) => d.name.toLowerCase().includes("pixel"));
        setSelectedDeviceId(pixel?.id ?? defs[0].id);
      }
      // selectedImageIdx stays 0 (highest API, already sorted).
    } catch (e) {
      setError(`Failed to load device data: ${formatError(e)}`);
    } finally {
      setLoading(false);
    }
  });

  async function handleCreate() {
    const n = nameToUse();
    const img = selectedImage();
    if (!n || !img || nameError()) return;

    setCreating(true);
    setError(null);
    try {
      const avds = await createAvdDevice(n, img.sdkId, selectedDeviceId() || undefined);
      props.onCreated(avds);
    } catch (err) {
      const e = err as { message?: string };
      setError(typeof err === "string" ? err : `Creation failed: ${e?.message ?? String(err)}`);
    } finally {
      setCreating(false);
    }
  }

  function handleKeyDown(e: KeyboardEvent) {
    if (e.key === "Escape") props.onClose();
    if (e.key === "Enter" && !creating() && !nameError() && nameToUse()) {
      handleCreate();
    }
  }

  return (
    <Portal>
      {/* Backdrop */}
      <div
        onKeyDown={handleKeyDown}
        onClick={() => props.onClose()}
        style={{
          position: "fixed",
          inset: "0",
          background: "rgba(0,0,0,0.55)",
          "z-index": "5000",
          display: "flex",
          "align-items": "center",
          "justify-content": "center",
        }}
      >
        {/* Dialog */}
        <div
          onClick={(e) => e.stopPropagation()}
          style={{
            background: "var(--bg-tertiary)",
            border: "1px solid var(--border)",
            "border-radius": "10px",
            width: "520px",
            "max-width": "calc(100vw - 48px)",
            "max-height": "calc(100vh - 80px)",
            "box-shadow": "0 8px 40px rgba(0,0,0,0.6)",
            display: "flex",
            "flex-direction": "column",
            overflow: "hidden",
          }}
        >
          {/* Header */}
          <div
            style={{
              display: "flex",
              "align-items": "center",
              padding: "16px 20px 14px",
              "border-bottom": "1px solid var(--border)",
              gap: "10px",
              "flex-shrink": "0",
            }}
          >
            <div
              style={{
                width: "32px",
                height: "32px",
                "border-radius": "8px",
                background: "rgba(var(--accent-rgb, 99,102,241), 0.15)",
                border: "1px solid rgba(var(--accent-rgb, 99,102,241), 0.3)",
                display: "flex",
                "align-items": "center",
                "justify-content": "center",
                "flex-shrink": "0",
              }}
            >
              <Icon name="device" size={16} color="var(--accent)" />
            </div>
            <div style={{ flex: "1" }}>
              <div style={{ "font-size": "14px", "font-weight": "600", color: "var(--text-primary)" }}>
                Create Virtual Device
              </div>
              <div style={{ "font-size": "11px", color: "var(--text-muted)", "margin-top": "1px" }}>
                Configure a new Android emulator
              </div>
            </div>
            <button
              onClick={() => props.onClose()}
              style={{
                background: "none", border: "none", cursor: "pointer",
                color: "var(--text-muted)", "font-size": "16px",
                padding: "4px", "border-radius": "4px",
                display: "flex", "align-items": "center",
              }}
              onMouseEnter={(e) => { (e.currentTarget as HTMLElement).style.background = "var(--bg-hover, rgba(255,255,255,0.08))"; }}
              onMouseLeave={(e) => { (e.currentTarget as HTMLElement).style.background = "none"; }}
            >
              ✕
            </button>
          </div>

          {/* Body */}
          <Show when={loading()}>
            <div style={{ padding: "40px 20px", "text-align": "center", color: "var(--text-muted)", "font-size": "13px" }}>
              <div class="lsp-spinner" style={{ display: "inline-block", "margin-bottom": "10px" }}>
                <Icon name="spinner" size={20} color="var(--accent)" />
              </div>
              <div>Loading available system images…</div>
            </div>
          </Show>

          <Show when={!loading() && images().length === 0}>
            <div style={{ padding: "32px 24px", "text-align": "center" }}>
              <Icon name="warning" size={32} color="var(--warning, #fbbf24)" />
              <div style={{ "font-size": "14px", "font-weight": "500", color: "var(--text-secondary)", "margin-top": "12px", "margin-bottom": "8px" }}>
                No system images found
              </div>
              <div style={{ "font-size": "12px", color: "var(--text-muted)", "line-height": "1.6", "max-width": "340px", margin: "0 auto" }}>
                Download system images via the Android SDK Manager in Android Studio, or run{" "}
                <code style={{ "font-family": "monospace", background: "var(--bg-primary)", padding: "1px 4px", "border-radius": "3px" }}>
                  sdkmanager "system-images;android-35;google_apis;arm64-v8a"
                </code>
              </div>
            </div>
          </Show>

          <Show when={!loading() && images().length > 0}>
            <div style={{ padding: "20px", display: "flex", "flex-direction": "column", gap: "16px", "overflow-y": "auto", flex: "1" }}>

              {/* Device Profile */}
              <FormGroup label="Device Profile" hint="Hardware configuration (screen size, sensors)">
                <Show
                  when={deviceDefs().length > 0}
                  fallback={
                    <div style={{ "font-size": "12px", color: "var(--text-muted)", "font-style": "italic" }}>
                      No device definitions found — a generic AVD will be created.
                    </div>
                  }
                >
                  <select
                    value={selectedDeviceId()}
                    onChange={(e) => setSelectedDeviceId(e.currentTarget.value)}
                    style={selectStyle()}
                  >
                    <For each={deviceDefs()}>
                      {(def) => (
                        <option value={def.id}>
                          {def.name}{def.manufacturer ? ` (${def.manufacturer})` : ""}
                        </option>
                      )}
                    </For>
                  </select>
                </Show>
              </FormGroup>

              {/* System Image */}
              <FormGroup label="System Image" hint="Android version and CPU architecture">
                <select
                  value={selectedImageIdx().toString()}
                  onChange={(e) => setSelectedImageIdx(parseInt(e.currentTarget.value))}
                  style={selectStyle()}
                >
                  <For each={images()}>
                    {(img, i) => (
                      <option value={i().toString()}>
                        {img.displayName}
                      </option>
                    )}
                  </For>
                </select>
                <Show when={selectedImage()}>
                  {(img) => (
                    <div style={{ display: "flex", gap: "6px", "margin-top": "6px", "flex-wrap": "wrap" }}>
                      <Badge label={`API ${img().apiLevel}`} />
                      <Badge label={img().abi} />
                      <Badge label={img().variant.replace(/_/g, " ")} />
                    </div>
                  )}
                </Show>
              </FormGroup>

              {/* AVD Name */}
              <FormGroup
                label="AVD Name"
                hint="Identifier used to refer to this device"
                error={nameError() ?? undefined}
              >
                <input
                  type="text"
                  value={nameToUse()}
                  placeholder={suggestedName() || "my_device"}
                  onInput={(e) => {
                    setNameManuallyEdited(true);
                    setName(e.currentTarget.value);
                  }}
                  style={{
                    ...inputStyle(),
                    "border-color": nameError() ? "var(--error, #f87171)" : undefined,
                  }}
                />
              </FormGroup>

              {/* Error */}
              <Show when={error()}>
                <div
                  style={{
                    padding: "10px 12px",
                    background: "rgba(248,113,113,0.1)",
                    border: "1px solid rgba(248,113,113,0.3)",
                    "border-radius": "6px",
                    "font-size": "12px",
                    color: "var(--error, #f87171)",
                    "line-height": "1.5",
                  }}
                >
                  {error()}
                </div>
              </Show>
            </div>
          </Show>

          {/* Footer */}
          <Show when={!loading() && images().length > 0}>
            <div
              style={{
                display: "flex",
                gap: "8px",
                "justify-content": "flex-end",
                padding: "14px 20px",
                "border-top": "1px solid var(--border)",
                "flex-shrink": "0",
              }}
            >
              <button
                onClick={() => props.onClose()}
                style={{
                  padding: "7px 16px",
                  background: "transparent",
                  border: "1px solid var(--border)",
                  "border-radius": "5px",
                  color: "var(--text-secondary)",
                  "font-size": "13px",
                  cursor: "pointer",
                }}
                onMouseEnter={(e) => { (e.currentTarget as HTMLElement).style.borderColor = "var(--text-muted)"; }}
                onMouseLeave={(e) => { (e.currentTarget as HTMLElement).style.borderColor = "var(--border)"; }}
              >
                Cancel
              </button>
              <button
                onClick={handleCreate}
                disabled={creating() || !!nameError() || !nameToUse()}
                style={{
                  padding: "7px 18px",
                  background: "var(--accent)",
                  border: "1px solid transparent",
                  "border-radius": "5px",
                  color: "#fff",
                  "font-size": "13px",
                  "font-weight": "500",
                  cursor: creating() || !!nameError() || !nameToUse() ? "default" : "pointer",
                  opacity: creating() || !!nameError() || !nameToUse() ? "0.55" : "1",
                  display: "flex",
                  "align-items": "center",
                  gap: "6px",
                  transition: "opacity 0.1s",
                }}
              >
                <Show when={creating()}>
                  <span class="lsp-spinner">
                    <Icon name="spinner" size={13} color="rgba(255,255,255,0.8)" />
                  </span>
                </Show>
                {creating() ? "Creating…" : "Create Device"}
              </button>
            </div>
          </Show>
        </div>
      </div>
    </Portal>
  );
}

// ── Sub-components ─────────────────────────────────────────────────────────────

function FormGroup(props: {
  label: string;
  hint?: string;
  error?: string;
  children: JSX.Element;
}): JSX.Element {
  return (
    <div style={{ display: "flex", "flex-direction": "column", gap: "6px" }}>
      <div style={{ display: "flex", "align-items": "baseline", gap: "6px" }}>
        <label style={{ "font-size": "12px", "font-weight": "600", color: "var(--text-secondary)" }}>
          {props.label}
        </label>
        <Show when={props.hint}>
          <span style={{ "font-size": "11px", color: "var(--text-muted)" }}>{props.hint}</span>
        </Show>
      </div>
      {props.children}
      <Show when={props.error}>
        <span style={{ "font-size": "11px", color: "var(--error, #f87171)" }}>{props.error}</span>
      </Show>
    </div>
  );
}

function Badge(props: { label: string }): JSX.Element {
  return (
    <span
      style={{
        "font-size": "10px",
        padding: "2px 6px",
        "border-radius": "4px",
        background: "var(--bg-primary, rgba(255,255,255,0.06))",
        color: "var(--text-muted)",
        border: "1px solid var(--border)",
      }}
    >
      {props.label}
    </span>
  );
}

// ── Style helpers ──────────────────────────────────────────────────────────────

function inputStyle(): Record<string, string> {
  return {
    width: "100%",
    padding: "7px 10px",
    background: "var(--bg-secondary)",
    border: "1px solid var(--border)",
    "border-radius": "5px",
    color: "var(--text-primary)",
    "font-size": "13px",
    outline: "none",
    "box-sizing": "border-box",
  };
}

function selectStyle(): Record<string, string> {
  return {
    width: "100%",
    padding: "7px 10px",
    background: "var(--bg-secondary)",
    border: "1px solid var(--border)",
    "border-radius": "5px",
    color: "var(--text-primary)",
    "font-size": "13px",
    outline: "none",
    cursor: "pointer",
    appearance: "none",
    "-webkit-appearance": "none",
    "box-sizing": "border-box",
  };
}
