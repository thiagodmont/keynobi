/**
 * DownloadSystemImageDialog.tsx
 *
 * Lists all Android system images available through sdkmanager.
 * Allows filtering by API level or name, and downloads with live progress.
 * Installed images are marked; not-installed images show a Download button.
 */

import {
  type JSX,
  Show,
  For,
  createSignal,
  createMemo,
  onMount,
} from "solid-js";
import { Portal } from "solid-js/web";
import type { AvailableSystemImage } from "@/bindings";
import { listAvailableSystemImages, downloadSystemImage } from "@/lib/tauri-api";
import Icon from "@/components/common/Icon";

export interface DownloadSystemImageDialogProps {
  onClose: () => void;
  /** Called after a download completes so the installed image list can refresh */
  onDownloaded: () => void;
}

interface DownloadState {
  sdkId: string;
  percent: number | null;
  message: string;
  done: boolean;
  error: boolean;
}

export function DownloadSystemImageDialog(
  props: DownloadSystemImageDialogProps
): JSX.Element {
  const [loading, setLoading] = createSignal(true);
  const [loadError, setLoadError] = createSignal<string | null>(null);
  const [images, setImages] = createSignal<AvailableSystemImage[]>([]);
  const [filter, setFilter] = createSignal("");
  const [downloading, setDownloading] = createSignal<DownloadState | null>(null);

  onMount(async () => {
    try {
      const list = await listAvailableSystemImages();
      setImages(list);
    } catch (e: any) {
      setLoadError(typeof e === "string" ? e : `Failed to fetch image list: ${e?.message ?? e}`);
    } finally {
      setLoading(false);
    }
  });

  const filtered = createMemo(() => {
    const q = filter().toLowerCase().trim();
    if (!q) return images();
    return images().filter(
      (img) =>
        img.displayName.toLowerCase().includes(q) ||
        img.sdkId.toLowerCase().includes(q) ||
        String(img.apiLevel).includes(q)
    );
  });

  async function handleDownload(img: AvailableSystemImage) {
    setDownloading({
      sdkId: img.sdkId,
      percent: null,
      message: "Starting download…",
      done: false,
      error: false,
    });

    try {
      await downloadSystemImage(img.sdkId, (progress) => {
        setDownloading((prev) => ({
          sdkId: img.sdkId,
          percent: progress.percent ?? prev?.percent ?? null,
          message: progress.message,
          done: progress.done,
          error: progress.error,
        }));

        if (progress.done && !progress.error) {
          // Mark this image as installed in the local list.
          setImages((prev) =>
            prev.map((i) => (i.sdkId === img.sdkId ? { ...i, installed: true } : i))
          );
          props.onDownloaded();
        }
      });
    } catch (e: any) {
      setDownloading((prev) =>
        prev
          ? { ...prev, done: true, error: true, message: typeof e === "string" ? e : `Error: ${e?.message ?? e}` }
          : null
      );
    }
  }

  function dismissDownload() {
    setDownloading(null);
  }

  return (
    <Portal>
      {/* Backdrop */}
      <div
        onClick={() => { if (!downloading() || downloading()!.done) props.onClose(); }}
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
            width: "560px",
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
                background: "rgba(99,102,241,0.15)",
                border: "1px solid rgba(99,102,241,0.3)",
                display: "flex",
                "align-items": "center",
                "justify-content": "center",
                "flex-shrink": "0",
              }}
            >
              <Icon name="download" size={16} color="var(--accent)" />
            </div>
            <div style={{ flex: "1" }}>
              <div style={{ "font-size": "14px", "font-weight": "600", color: "var(--text-primary)" }}>
                Download System Image
              </div>
              <div style={{ "font-size": "11px", color: "var(--text-muted)", "margin-top": "1px" }}>
                Browse and download Android system images via sdkmanager
              </div>
            </div>
            <button
              onClick={props.onClose}
              disabled={!!(downloading() && !downloading()!.done)}
              style={{
                background: "none", border: "none", cursor: "pointer",
                color: "var(--text-muted)", "font-size": "16px",
                padding: "4px", "border-radius": "4px", display: "flex",
                "align-items": "center",
                opacity: downloading() && !downloading()!.done ? "0.3" : "1",
              }}
              onMouseEnter={(e) => { if (!downloading() || downloading()!.done) (e.currentTarget as HTMLElement).style.background = "var(--bg-hover, rgba(255,255,255,0.08))"; }}
              onMouseLeave={(e) => { (e.currentTarget as HTMLElement).style.background = "none"; }}
            >
              ✕
            </button>
          </div>

          {/* Download progress overlay */}
          <Show when={downloading()}>
            {(dl) => (
              <div
                style={{
                  padding: "16px 20px",
                  "border-bottom": "1px solid var(--border)",
                  background: dl().error
                    ? "rgba(248,113,113,0.08)"
                    : dl().done
                    ? "rgba(74,222,128,0.08)"
                    : "rgba(99,102,241,0.08)",
                  "flex-shrink": "0",
                }}
              >
                <div style={{ display: "flex", "align-items": "center", "justify-content": "space-between", "margin-bottom": "8px" }}>
                  <span style={{ "font-size": "12px", "font-weight": "500", color: dl().error ? "var(--error, #f87171)" : dl().done ? "#4ade80" : "var(--text-primary)" }}>
                    {dl().error ? "Download failed" : dl().done ? "Download complete" : "Downloading…"}
                  </span>
                  <Show when={dl().done}>
                    <button
                      onClick={dismissDownload}
                      style={{ background: "none", border: "none", cursor: "pointer", color: "var(--text-muted)", "font-size": "11px" }}
                    >
                      Dismiss
                    </button>
                  </Show>
                </div>

                {/* Progress bar */}
                <Show when={!dl().done || dl().error}>
                  <div
                    style={{
                      height: "4px",
                      background: "var(--bg-primary)",
                      "border-radius": "2px",
                      overflow: "hidden",
                      "margin-bottom": "6px",
                    }}
                  >
                    <div
                      style={{
                        height: "100%",
                        width: dl().percent !== null && dl().percent !== undefined ? `${dl().percent}%` : "100%",
                        background: dl().error ? "var(--error, #f87171)" : "var(--accent)",
                        "border-radius": "2px",
                        transition: "width 0.3s ease",
                        animation: (dl().percent === null || dl().percent === undefined) && !dl().done ? "pulse 1.5s ease-in-out infinite" : "none",
                      }}
                    />
                  </div>
                </Show>

                <div style={{ "font-size": "11px", color: "var(--text-muted)", "word-break": "break-all" }}>
                  {dl().message}
                </div>
              </div>
            )}
          </Show>

          {/* Search filter */}
          <Show when={!loading() && !loadError()}>
            <div style={{ padding: "10px 20px", "border-bottom": "1px solid var(--border)", "flex-shrink": "0" }}>
              <div style={{ position: "relative" }}>
                <Icon
                  name="search"
                  size={13}
                  color="var(--text-muted)"
                  class="sidebar-search-icon"
                />
                <input
                  type="text"
                  placeholder="Filter by name, API level, or ABI…"
                  value={filter()}
                  onInput={(e) => setFilter(e.currentTarget.value)}
                  style={{
                    width: "100%",
                    padding: "6px 10px 6px 28px",
                    background: "var(--bg-secondary)",
                    border: "1px solid var(--border)",
                    "border-radius": "5px",
                    color: "var(--text-primary)",
                    "font-size": "12px",
                    outline: "none",
                    "box-sizing": "border-box",
                  }}
                />
              </div>
            </div>
          </Show>

          {/* Body */}
          <div style={{ flex: "1", "overflow-y": "auto" }}>
            {/* Loading state */}
            <Show when={loading()}>
              <div style={{ padding: "40px 20px", "text-align": "center", color: "var(--text-muted)", "font-size": "13px" }}>
                <div class="lsp-spinner" style={{ display: "inline-block", "margin-bottom": "10px" }}>
                  <Icon name="spinner" size={20} color="var(--accent)" />
                </div>
                <div>Fetching available system images from sdkmanager…</div>
                <div style={{ "font-size": "11px", "margin-top": "6px", opacity: "0.7" }}>
                  This may take a few seconds on first run.
                </div>
              </div>
            </Show>

            {/* Load error */}
            <Show when={!loading() && loadError()}>
              <div style={{ padding: "32px 24px", "text-align": "center" }}>
                <Icon name="warning" size={28} color="var(--warning, #fbbf24)" />
                <div style={{ "font-size": "14px", "font-weight": "500", color: "var(--text-secondary)", "margin-top": "12px", "margin-bottom": "8px" }}>
                  Could not fetch image list
                </div>
                <div style={{ "font-size": "12px", color: "var(--text-muted)", "line-height": "1.6", "max-width": "360px", margin: "0 auto" }}>
                  {loadError()}
                  <br /><br />
                  Make sure <code style={{ "font-family": "monospace", background: "var(--bg-primary)", padding: "1px 4px", "border-radius": "3px" }}>sdkmanager</code> is installed in your Android SDK Command-Line Tools.
                </div>
              </div>
            </Show>

            {/* Empty filter result */}
            <Show when={!loading() && !loadError() && filtered().length === 0 && images().length > 0}>
              <div style={{ padding: "32px 24px", "text-align": "center", color: "var(--text-muted)", "font-size": "13px" }}>
                No system images match "{filter()}"
              </div>
            </Show>

            {/* Image list */}
            <Show when={!loading() && !loadError() && filtered().length > 0}>
              <div>
                <For each={filtered()}>
                  {(img) => (
                    <SystemImageRow
                      image={img}
                      isActiveDownload={downloading()?.sdkId === img.sdkId && !downloading()!.done}
                      onDownload={() => handleDownload(img)}
                    />
                  )}
                </For>
              </div>
            </Show>
          </div>

          {/* Footer */}
          <div
            style={{
              padding: "10px 20px",
              "border-top": "1px solid var(--border)",
              "flex-shrink": "0",
              display: "flex",
              "justify-content": "space-between",
              "align-items": "center",
            }}
          >
            <span style={{ "font-size": "11px", color: "var(--text-muted)" }}>
              <Show when={!loading() && !loadError()}>
                {filtered().length} of {images().length} images
                {" · "}
                {images().filter((i) => i.installed).length} installed
              </Show>
            </span>
            <button
              onClick={props.onClose}
              disabled={!!(downloading() && !downloading()!.done)}
              style={{
                padding: "6px 16px",
                background: "transparent",
                border: "1px solid var(--border)",
                "border-radius": "5px",
                color: "var(--text-secondary)",
                "font-size": "13px",
                cursor: downloading() && !downloading()!.done ? "default" : "pointer",
                opacity: downloading() && !downloading()!.done ? "0.4" : "1",
              }}
            >
              Close
            </button>
          </div>
        </div>
      </div>
    </Portal>
  );
}

// ── System image row ──────────────────────────────────────────────────────────

function SystemImageRow(props: {
  image: AvailableSystemImage;
  isActiveDownload: boolean;
  onDownload: () => void;
}): JSX.Element {
  return (
    <div
      style={{
        display: "flex",
        "align-items": "center",
        padding: "9px 20px",
        gap: "12px",
        "border-bottom": "1px solid var(--border)",
        background: props.isActiveDownload ? "rgba(99,102,241,0.06)" : "transparent",
        transition: "background 0.1s",
      }}
      onMouseEnter={(e) => {
        if (!props.isActiveDownload && !props.image.installed)
          (e.currentTarget as HTMLElement).style.background = "var(--bg-hover, rgba(255,255,255,0.03))";
      }}
      onMouseLeave={(e) => {
        if (!props.isActiveDownload)
          (e.currentTarget as HTMLElement).style.background = "transparent";
      }}
    >
      {/* API level badge */}
      <div
        style={{
          width: "36px",
          height: "36px",
          "border-radius": "8px",
          background: props.image.installed
            ? "rgba(74,222,128,0.12)"
            : "var(--bg-primary, rgba(255,255,255,0.05))",
          border: `1px solid ${props.image.installed ? "rgba(74,222,128,0.25)" : "var(--border)"}`,
          display: "flex",
          "align-items": "center",
          "justify-content": "center",
          "flex-shrink": "0",
        }}
      >
        <span style={{ "font-size": "11px", "font-weight": "600", color: props.image.installed ? "#4ade80" : "var(--text-muted)" }}>
          {props.image.apiLevel}
        </span>
      </div>

      {/* Image info */}
      <div style={{ flex: "1", "min-width": "0" }}>
        <div style={{ "font-size": "12px", "font-weight": "500", color: "var(--text-primary)", overflow: "hidden", "text-overflow": "ellipsis", "white-space": "nowrap" }}>
          {props.image.displayName}
        </div>
        <div style={{ "font-size": "10px", color: "var(--text-muted)", "margin-top": "2px", overflow: "hidden", "text-overflow": "ellipsis", "white-space": "nowrap" }}>
          {props.image.sdkId}
        </div>
      </div>

      {/* Status / action */}
      <div style={{ "flex-shrink": "0" }}>
        <Show
          when={props.image.installed}
          fallback={
            <Show
              when={!props.isActiveDownload}
              fallback={
                <span class="lsp-spinner">
                  <Icon name="spinner" size={14} color="var(--accent)" />
                </span>
              }
            >
              <button
                onClick={props.onDownload}
                style={{
                  display: "flex",
                  "align-items": "center",
                  gap: "5px",
                  padding: "4px 10px",
                  background: "transparent",
                  border: "1px solid var(--border)",
                  "border-radius": "4px",
                  color: "var(--text-secondary)",
                  "font-size": "11px",
                  "font-weight": "500",
                  cursor: "pointer",
                }}
                onMouseEnter={(e) => {
                  (e.currentTarget as HTMLElement).style.borderColor = "var(--accent)";
                  (e.currentTarget as HTMLElement).style.color = "var(--accent)";
                }}
                onMouseLeave={(e) => {
                  (e.currentTarget as HTMLElement).style.borderColor = "var(--border)";
                  (e.currentTarget as HTMLElement).style.color = "var(--text-secondary)";
                }}
              >
                <Icon name="download" size={11} />
                Download
              </button>
            </Show>
          }
        >
          <span
            style={{
              display: "flex",
              "align-items": "center",
              gap: "4px",
              "font-size": "11px",
              color: "#4ade80",
              "font-weight": "500",
            }}
          >
            <span style={{ "font-size": "13px" }}>✓</span>
            Installed
          </span>
        </Show>
      </div>
    </div>
  );
}
