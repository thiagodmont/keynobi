import { type JSX, For, Show, createMemo, createSignal } from "solid-js";
import type { UiNode } from "@/bindings";
import {
  pickNodePathAtDevicePoint,
  prepareWireframeDrawList,
  shortClassName,
} from "@/lib/ui-hierarchy-display";

export interface LayoutWireframeProps {
  root: UiNode;
  selectedPath: string | null;
  onSelectPath: (path: string) => void;
  /** Base64-encoded PNG screenshot to overlay behind the wireframe rects. */
  screenshotB64?: string;
}

function clientToDevice(
  svg: SVGSVGElement,
  clientX: number,
  clientY: number
): { x: number; y: number } | null {
  const pt = svg.createSVGPoint();
  pt.x = clientX;
  pt.y = clientY;
  const ctm = svg.getScreenCTM();
  if (!ctm) return null;
  const p = pt.matrixTransform(ctm.inverse());
  return { x: p.x, y: p.y };
}

const LEGEND_ITEMS = [
  { color: "#60a5fa", label: "Interactive" },
  { color: "#2dd4bf", label: "Text" },
  { color: "#f59e0b", label: "Compose" },
  { color: "var(--border)", label: "Container" },
] as const;

const MIN_ZOOM = 0.2;
const MAX_ZOOM = 10;
const ZOOM_FACTOR = 1.15;

// Color-coding by node role (priority: interactive > text > compose > container)
function nodeStrokeColor(node: UiNode): string {
  if (node.clickable || node.editable) return "#60a5fa"; // interactive → blue
  if (node.text || node.contentDesc) return "#2dd4bf";   // text content → teal
  if (node.isComposeHeuristic) return "#f59e0b";         // Compose → amber
  return "var(--border)";                                // container → default
}

function nodeFillColor(node: UiNode): string {
  if (node.clickable || node.editable) return "rgba(96,165,250,0.07)";
  if (node.text || node.contentDesc) return "rgba(45,212,191,0.06)";
  if (node.isComposeHeuristic) return "rgba(245,158,11,0.06)";
  return "rgba(148,163,184,0.06)";
}

export function LayoutWireframe(props: LayoutWireframeProps): JSX.Element {
  const prepared = createMemo(() => prepareWireframeDrawList(props.root));
  const [hoverPath, setHoverPath] = createSignal<string | null>(null);
  const [showScreenshot, setShowScreenshot] = createSignal(false);
  const [zoom, setZoom] = createSignal(1);
  const [pan, setPan] = createSignal({ x: 0, y: 0 });
  const [dragging, setDragging] = createSignal(false);
  let dragStart = { x: 0, y: 0 };
  let panStart = { x: 0, y: 0 };

  const hasScreenshot = () => !!props.screenshotB64;
  const screenshotDataUrl = () =>
    props.screenshotB64 ? `data:image/png;base64,${props.screenshotB64}` : null;

  const resetZoom = (): void => {
    setZoom(1);
    setPan({ x: 0, y: 0 });
  };

  const viewBox = createMemo(() => {
    const { width, height } = prepared().screen;
    const z = zoom();
    const p = pan();
    const vw = width / z;
    const vh = height / z;
    return `${p.x} ${p.y} ${vw} ${vh}`;
  });

  const hitTest = (svg: SVGSVGElement, clientX: number, clientY: number): string | null => {
    const d = clientToDevice(svg, clientX, clientY);
    if (!d) return null;
    return pickNodePathAtDevicePoint(prepared().entries, d.x, d.y);
  };

  return (
    <div
      style={{
        display: "flex",
        "flex-direction": "column",
        height: "100%",
        "min-height": "0",
        "min-width": "240px",
        "max-width": "40vw",
        "flex-shrink": 0,
        padding: "8px",
        "border-right": "1px solid var(--border)",
        background: "var(--bg-primary)",
      }}
    >
      <div
        style={{
          display: "flex",
          "align-items": "center",
          gap: "8px",
          "margin-bottom": "6px",
          "flex-shrink": 0,
        }}
      >
        <span
          style={{
            "font-size": "11px",
            "font-weight": "600",
            color: "var(--text-muted)",
            flex: "1",
          }}
        >
          Wireframe (device px)
        </span>
        <Show when={zoom() !== 1 || pan().x !== 0 || pan().y !== 0}>
          <button
            type="button"
            title="Reset zoom and pan"
            onClick={resetZoom}
            style={{
              background: "var(--bg-secondary)",
              color: "var(--text-muted)",
              border: "1px solid var(--border)",
              "border-radius": "4px",
              padding: "2px 7px",
              "font-size": "10px",
              cursor: "pointer",
            }}
          >
            Reset
          </button>
        </Show>
        <Show when={hasScreenshot()}>
          <button
            type="button"
            title={showScreenshot() ? "Hide screenshot overlay" : "Show screenshot overlay"}
            onClick={() => setShowScreenshot((v) => !v)}
            style={{
              background: showScreenshot() ? "var(--accent)" : "var(--bg-secondary)",
              color: showScreenshot() ? "var(--bg-primary)" : "var(--text-muted)",
              border: "1px solid var(--border)",
              "border-radius": "4px",
              padding: "2px 7px",
              "font-size": "10px",
              cursor: "pointer",
            }}
          >
            {showScreenshot() ? "Hide screenshot" : "Show screenshot"}
          </button>
        </Show>
      </div>
      <Show when={prepared().truncated}>
        <div
          style={{
            "font-size": "10px",
            color: "var(--warning, #fbbf24)",
            "margin-bottom": "6px",
            "flex-shrink": 0,
          }}
        >
          Showing first {prepared().entries.length} rects (cap). Refine filters or use search to reduce
          the tree.
        </div>
      </Show>
      <div style={{ flex: "1", "min-height": "0", position: "relative" }}>
        <svg
          role="img"
          aria-label="UI bounds wireframe"
          style={{
            width: "100%",
            height: "100%",
            display: "block",
            cursor: dragging() ? "grabbing" : "crosshair",
            "user-select": "none",
          }}
          viewBox={viewBox()}
          preserveAspectRatio="xMidYMid meet"
          onDblClick={resetZoom}
          onWheel={(e) => {
            e.preventDefault();
            const { width, height } = prepared().screen;
            const svg = e.currentTarget;
            const rect = svg.getBoundingClientRect();
            // Device-space point under the cursor before zoom.
            const z0 = zoom();
            const p0 = pan();
            const vw0 = width / z0;
            const vh0 = height / z0;
            const cx = p0.x + ((e.clientX - rect.left) / rect.width) * vw0;
            const cy = p0.y + ((e.clientY - rect.top) / rect.height) * vh0;
            const newZoom = Math.min(MAX_ZOOM, Math.max(MIN_ZOOM,
              e.deltaY < 0 ? z0 * ZOOM_FACTOR : z0 / ZOOM_FACTOR
            ));
            const vwNew = width / newZoom;
            const vhNew = height / newZoom;
            // Anchor the device point under cursor.
            const newPx = cx - ((e.clientX - rect.left) / rect.width) * vwNew;
            const newPy = cy - ((e.clientY - rect.top) / rect.height) * vhNew;
            setZoom(newZoom);
            setPan({ x: newPx, y: newPy });
          }}
          onMouseDown={(e) => {
            if (e.button !== 0) return;
            // Only drag when not clicking on a node (middle/background click).
            setDragging(true);
            dragStart = { x: e.clientX, y: e.clientY };
            panStart = pan();
          }}
          onMouseMove={(e) => {
            if (dragging()) {
              const { width, height } = prepared().screen;
              const svg = e.currentTarget;
              const rect = svg.getBoundingClientRect();
              const z = zoom();
              const dx = ((dragStart.x - e.clientX) / rect.width) * (width / z);
              const dy = ((dragStart.y - e.clientY) / rect.height) * (height / z);
              setPan({ x: panStart.x + dx, y: panStart.y + dy });
            }
            if (!dragging()) {
              const svg = e.currentTarget;
              setHoverPath(hitTest(svg, e.clientX, e.clientY));
            }
          }}
          onMouseUp={(e) => {
            if (!dragging()) return;
            const moved = Math.abs(e.clientX - dragStart.x) + Math.abs(e.clientY - dragStart.y);
            setDragging(false);
            if (moved < 4) {
              // Treat as click — select node.
              const svg = e.currentTarget;
              const path = hitTest(svg, e.clientX, e.clientY);
              if (path !== null) props.onSelectPath(path);
            }
          }}
          onMouseLeave={() => {
            setDragging(false);
            setHoverPath(null);
          }}
        >
          {/* Background: screenshot when visible, solid fill otherwise */}
          <Show
            when={showScreenshot() && screenshotDataUrl()}
            fallback={
              <rect
                x="0"
                y="0"
                width={prepared().screen.width}
                height={prepared().screen.height}
                fill="var(--bg-secondary)"
                stroke="var(--border)"
                stroke-width="1"
                style={{ "pointer-events": "none" }}
              />
            }
          >
            <image
              href={screenshotDataUrl()!}
              x="0"
              y="0"
              width={prepared().screen.width}
              height={prepared().screen.height}
              preserveAspectRatio="xMidYMid meet"
              style={{ "pointer-events": "none" }}
            />
          </Show>
          <For each={prepared().drawOrder}>
            {(entry) => {
              const { left, top, right, bottom } = entry.rect;
              const w = right - left;
              const h = bottom - top;
              const isSel = () => props.selectedPath === entry.path;
              const isHov = () => hoverPath() === entry.path;
              const sw = () => (isSel() ? 3 : isHov() ? 2 : 0.5);
              // When the screenshot is showing, use semi-transparent overlays so the
              // image remains readable underneath.
              const overlayMode = () => showScreenshot() && hasScreenshot();
              return (
                <g>
                  <title>
                    {entry.path || "(root)"} — {shortClassName(entry.node.class)}
                    {entry.node.resourceId ? ` — ${entry.node.resourceId}` : ""}
                  </title>
                  <rect
                    x={left}
                    y={top}
                    width={w}
                    height={h}
                    fill={
                      isSel()
                        ? "rgba(96,165,250,0.25)"
                        : isHov()
                          ? "rgba(148,163,184,0.18)"
                          : overlayMode()
                            ? "transparent"
                            : nodeFillColor(entry.node)
                    }
                    stroke={
                      isSel()
                        ? "var(--accent)"
                        : isHov()
                          ? "var(--text-muted)"
                          : nodeStrokeColor(entry.node)
                    }
                    stroke-width={sw()}
                    style={{ "pointer-events": "none" }}
                  />
                </g>
              );
            }}
          </For>
        </svg>
      </div>
      <div
        style={{
          display: "flex",
          "flex-wrap": "wrap",
          gap: "6px 12px",
          "align-items": "center",
          "margin-top": "6px",
          "flex-shrink": 0,
        }}
      >
        {/* Color legend */}
        <For each={LEGEND_ITEMS}>
          {(item) => (
            <span
              style={{
                display: "inline-flex",
                "align-items": "center",
                gap: "4px",
                "font-size": "10px",
                color: "var(--text-muted)",
              }}
            >
              <span
                style={{
                  display: "inline-block",
                  width: "10px",
                  height: "10px",
                  border: `2px solid ${item.color}`,
                  "border-radius": "2px",
                  background: "transparent",
                  "flex-shrink": "0",
                }}
              />
              {item.label}
            </span>
          )}
        </For>
        <span
          style={{ "font-size": "10px", color: "var(--text-muted)", "margin-left": "auto" }}
        >
          Click to select
        </span>
      </div>
    </div>
  );
}
