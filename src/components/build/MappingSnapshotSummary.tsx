import { type JSX, For } from "solid-js";
import type { MappingSnapshot } from "@/bindings";
import { Badge } from "@/components/ui";

/** Characters of a map id shown; the full id is in the tooltip. */
const MAP_ID_SHOWN = 7;

function shortMapId(id: string): string {
  return id.length > MAP_ID_SHOWN ? `${id.slice(0, MAP_ID_SHOWN)}…` : id;
}

function formatBytes(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}

/** "R8 mapping saved: release (map id 6b1c2f0)"; the module is named when a build has several. */
export function mappingLabel(mapping: MappingSnapshot, withModule: boolean): string {
  const where = withModule ? `${mapping.module} ${mapping.variant}` : mapping.variant;
  const id = mapping.pgMapId ? ` (map id ${shortMapId(mapping.pgMapId)})` : "";
  return `R8 mapping saved: ${where}${id}`;
}

function mappingDetails(mapping: MappingSnapshot): string {
  return [
    `${mapping.module} ${mapping.variant}`,
    mapping.pgMapId ? `map id ${mapping.pgMapId}` : "no map id",
    `SHA-256 ${mapping.sha256.slice(0, 12)}…`,
    formatBytes(mapping.bytes),
  ].join(" · ");
}

/** One badge per R8 mapping Keynobi saved for a past build. */
export function MappingSnapshotSummary(props: {
  mappings: readonly MappingSnapshot[];
}): JSX.Element {
  const withModule = () => new Set(props.mappings.map((m) => m.module)).size > 1;
  return (
    <For each={props.mappings}>
      {(mapping) => (
        <Badge variant="success" size="xs" subtle title={mappingDetails(mapping)}>
          {mappingLabel(mapping, withModule())}
        </Badge>
      )}
    </For>
  );
}
