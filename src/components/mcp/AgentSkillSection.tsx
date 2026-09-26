import { type JSX, Show, createSignal, onMount } from "solid-js";
import type { AgentSkillState, AgentSkillStatus } from "@/bindings";
import { formatError, getAgentSkillStatus, installAgentSkill } from "@/lib/tauri-api";
import {
  Badge,
  Button,
  CopyableText,
  showDialog,
  showToast,
  type BadgeVariant,
} from "@/components/ui";
import styles from "./AgentSkillSection.module.css";

const STATE_LABEL: Record<AgentSkillState, string> = {
  notInstalled: "Not installed",
  installed: "Installed",
  different: "A different SKILL.md is there",
};

const STATE_BADGE: Record<AgentSkillState, BadgeVariant> = {
  notInstalled: "default",
  installed: "success",
  different: "warning",
};

/**
 * The Keynobi agent skill: install it for Claude Code on an explicit click,
 * or copy it for other clients. Nothing is written until the user clicks.
 */
export function AgentSkillSection(): JSX.Element {
  const [status, setStatus] = createSignal<AgentSkillStatus | null>(null);
  const [showContent, setShowContent] = createSignal(false);
  const [installing, setInstalling] = createSignal(false);

  const refresh = async () => {
    try {
      setStatus(await getAgentSkillStatus());
    } catch {
      setStatus(null);
    }
  };

  onMount(refresh);

  const install = async (current: AgentSkillStatus) => {
    const replace = current.state === "different";
    if (replace) {
      const choice = await showDialog({
        title: "Replace SKILL.md?",
        message: `${current.path} already exists and is not Keynobi's current skill. Replace it?`,
        buttons: [
          { label: "Replace", value: "replace", style: "danger" },
          { label: "Cancel", value: "cancel", style: "secondary" },
        ],
      });
      if (choice !== "replace") return;
    }
    setInstalling(true);
    try {
      setStatus(await installAgentSkill(replace));
      showToast("Keynobi skill installed for Claude Code", "success");
    } catch (err) {
      showToast(`Could not install the skill: ${formatError(err)}`, "error");
      await refresh();
    } finally {
      setInstalling(false);
    }
  };

  const copyContent = (content: string) => {
    navigator.clipboard
      ?.writeText(content)
      .then(() => showToast("SKILL.md copied", "success"))
      .catch(() => showToast("Could not copy SKILL.md", "error"));
  };

  return (
    <Show when={status()}>
      {(current) => (
        <section class={styles.root} aria-label="Keynobi agent skill">
          <div class={styles.heading}>
            Agent skill
            <Badge size="xs" variant={STATE_BADGE[current().state]}>
              {STATE_LABEL[current().state]}
            </Badge>
          </div>
          <p class={styles.text}>
            Tells AI agents when to use Keynobi and when to use Android CLI. Any MCP client can read
            it as the <span class={styles.code}>{current().resourceUri}</span> resource. Installing
            writes one file for Claude Code:
          </p>
          <div class={styles.target}>
            <CopyableText text={current().path} mono truncate />
          </div>
          <div class={styles.actions}>
            <Show when={current().state !== "installed"}>
              <Button
                size="xs"
                variant={current().state === "different" ? "danger" : "primary"}
                loading={installing()}
                onClick={() => void install(current())}
              >
                {current().state === "different"
                  ? "Replace for Claude Code…"
                  : "Install for Claude Code"}
              </Button>
            </Show>
            <Button size="xs" variant="outline" onClick={() => copyContent(current().content)}>
              Copy SKILL.md
            </Button>
            <Button
              size="xs"
              variant="ghost"
              ariaPressed={showContent()}
              onClick={() => setShowContent((v) => !v)}
            >
              {showContent() ? "Hide SKILL.md" : "Show SKILL.md"}
            </Button>
          </div>
          <Show when={showContent()}>
            <pre class={styles.preview} aria-label="SKILL.md">
              {current().content}
            </pre>
          </Show>
        </section>
      )}
    </Show>
  );
}
