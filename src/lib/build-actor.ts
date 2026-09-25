import type { BuildActor } from "@/bindings";

/** "an agent (Claude Code)", or "an agent" when the client did not say its name. */
function agentLabel(actor: Extract<BuildActor, { kind: "agent" }>): string {
  return actor.clientName ? `an agent (${actor.clientName})` : "an agent";
}

export function isAgent(actor: BuildActor | null | undefined): boolean {
  return actor?.kind === "agent";
}

/** Who started a build, for the Build panel: "Started by an agent (Claude Code)". */
export function startedByLabel(actor: BuildActor | null | undefined): string | null {
  if (!actor) return null;
  if (actor.kind === "agent") return `Started by ${agentLabel(actor)}`;
  return "Started in Keynobi";
}

/** Who cancelled a build: "Cancelled by an agent (Claude Code)", "Cancelled in Keynobi". */
export function cancelledByLabel(actor: BuildActor | null | undefined): string | null {
  if (!actor) return null;
  switch (actor.kind) {
    case "agent":
      return `Cancelled by ${agentLabel(actor)}`;
    case "appQuit":
      return "Cancelled because Keynobi quit";
    case "app":
      return "Cancelled in Keynobi";
  }
}

/**
 * Who started a build, when an agent did, and who cancelled it, unless the app
 * both started and cancelled it (the usual case, which needs no note).
 */
export function buildActorLabels(
  origin: BuildActor | null | undefined,
  cancelledBy: BuildActor | null | undefined
): string[] {
  const agentBuild = isAgent(origin);
  const labels: string[] = [];
  if (agentBuild) labels.push(startedByLabel(origin) ?? "");
  if (cancelledBy && (agentBuild || cancelledBy.kind !== "app")) {
    labels.push(cancelledByLabel(cancelledBy) ?? "");
  }
  return labels.filter(Boolean);
}

/** Why the app cannot build right now: "A build started by an agent (Claude Code) is running". */
export function buildRunningLabel(actor: BuildActor | null | undefined): string {
  return actor?.kind === "agent"
    ? `A build started by ${agentLabel(actor)} is running`
    : "A build is running";
}

/** Title of the cancel button, naming the agent whose build it stops. */
export function cancelBuildTitle(actor: BuildActor | null | undefined): string {
  return actor?.kind === "agent"
    ? `Cancel the build started by ${agentLabel(actor)}`
    : "Cancel build";
}
