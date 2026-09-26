import { type JSX, Match, Show, Switch } from "solid-js";
import { type BuildView, buildState, isBuilding } from "@/stores/build.store";
import { settingsState } from "@/stores/settings.store";
import { Alert, Button, Spinner } from "@/components/ui";
import { LogViewer } from "@/components/common/LogViewer";
import { relativeTime } from "@/components/build/BuildHistoryPanel";
import { buildRunningLabel } from "@/lib/build-actor";
import type { HistoricalLogState } from "./build-history-log";
import { LaunchTimingSummary } from "./LaunchTimingSummary";
import { MappingSnapshotSummary } from "./MappingSnapshotSummary";
import { InstalledBuildSummary } from "./InstalledBuildSummary";
import styles from "./BuildHistoryView.module.css";

export function formatBuildTime(iso: string): string {
  const date = new Date(iso);
  if (Number.isNaN(date.getTime())) return iso;
  return date.toLocaleString(undefined, { dateStyle: "medium", timeStyle: "short" });
}

/**
 * Says which past build the panel shows, offers the way back, and says when a
 * build is running meanwhile (it never replaces the past build by itself).
 */
export function HistoryViewBanner(props: { view: BuildView; onBack: () => void }): JSX.Element {
  const title = () => {
    const { id, startedAt } = props.view;
    return startedAt
      ? `Viewing build #${id} from ${formatBuildTime(startedAt)}`
      : `Viewing build #${id}`;
  };

  const launchedRecord = () =>
    buildState.history.find((r) => r.id === props.view.id && r.launch !== null);
  const savedMappings = () =>
    buildState.history.find((r) => r.id === props.view.id)?.mappings ?? [];
  const recordWithApks = () =>
    buildState.history.find((r) => r.id === props.view.id && r.apks.length > 0);

  return (
    <div class={styles.banner} data-testid="build-history-banner">
      <div class={styles.text}>
        <span class={styles.title}>{title()}</span>
        <Show when={props.view.task && props.view.startedAt}>
          <span class={styles.meta}>
            {props.view.task} · {relativeTime(props.view.startedAt ?? "")}
          </span>
        </Show>
        <Show when={launchedRecord()}>
          {(r) => (
            <span class={styles.meta}>
              <LaunchTimingSummary record={r()} history={buildState.history} />
            </span>
          )}
        </Show>
        <Show when={savedMappings().length > 0}>
          <MappingSnapshotSummary mappings={savedMappings()} />
        </Show>
        <Show when={recordWithApks()}>{(r) => <InstalledBuildSummary record={r()} />}</Show>
        <Show when={isBuilding()}>
          <span class={styles.running} role="status">
            {buildRunningLabel(buildState.origin)}
          </span>
        </Show>
      </div>
      <Button variant="outline" size="xs" onClick={() => props.onBack()}>
        {isBuilding() ? "Show running build" : "Back to current build"}
      </Button>
    </div>
  );
}

export function MissingBuildNotice(props: { id: number | null }): JSX.Element {
  return (
    <div class={styles.notice}>
      <Alert variant="info" title={`Build #${props.id} is no longer in the history`}>
        Keynobi keeps the last 10 builds of each project.
      </Alert>
    </div>
  );
}

function retentionText(): string {
  const days = settingsState.build.buildLogRetentionDays;
  const mb = settingsState.build.buildLogMaxFolderMb;
  const keep =
    days > 0
      ? `Keynobi keeps build logs for ${days} day${days === 1 ? "" : "s"} and removes the oldest when the build log folder passes ${mb} MB.`
      : `Keynobi removes the oldest build logs when the build log folder passes ${mb} MB.`;
  return `${keep} Change this under Settings → Advanced → Build.`;
}

/** The saved log of a past build: loading, removed by retention, unreadable, or shown. */
export function HistoricalLogView(props: {
  view: BuildView;
  state: HistoricalLogState;
  onRetry: () => void;
}): JSX.Element {
  const failure = () => (props.state.status === "failed" ? props.state.message : null);
  const entries = () => (props.state.status === "loaded" ? props.state.entries : null);

  return (
    <Switch>
      <Match when={props.view.missing}>
        <MissingBuildNotice id={props.view.id} />
      </Match>
      <Match when={props.state.status === "loading"}>
        <div class={styles.loading}>
          <Spinner size="sm" />
          <span>Loading this build's log…</span>
        </div>
      </Match>
      <Match when={props.state.status === "expired"}>
        <div class={styles.notice}>
          <Alert variant="info" title="This build's log was removed">
            {retentionText()}
          </Alert>
        </div>
      </Match>
      <Match when={failure()}>
        {(message) => (
          <div class={styles.notice}>
            <Alert
              variant="error"
              title="Couldn't load the log for this build"
              action={
                <Button variant="outline" size="xs" onClick={() => props.onRetry()}>
                  Retry
                </Button>
              }
            >
              {message()}
            </Alert>
          </div>
        )}
      </Match>
      <Match when={entries()}>
        {(lines) => (
          <LogViewer
            entries={lines()}
            defaultAutoScroll={settingsState.build.autoScrollBuildLog}
            showSource={false}
            emptyMessage="This build printed no output"
          />
        )}
      </Match>
    </Switch>
  );
}
