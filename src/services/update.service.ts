import { createStore } from "solid-js/store";
import { openUrl } from "@tauri-apps/plugin-opener";

const LATEST_RELEASE_URL = "https://api.github.com/repos/thiagodmont/keynobi/releases/latest";
const RELEASES_PAGE_URL = "https://github.com/thiagodmont/keynobi/releases";
const DISMISSED_UPDATE_TAG_KEY = "keynobi.dismissedUpdateTag";

export interface NormalizedRelease {
  tagName: string;
  version: string;
  name: string;
  releaseUrl: string;
  prerelease: boolean;
}

export interface AppUpdateInfo {
  available: boolean;
  dismissed: boolean;
  currentVersion: string;
  latestVersion?: string;
  tagName?: string;
  releaseName?: string;
  releaseUrl?: string;
  error?: string;
}

interface AppUpdateState {
  checking: boolean;
  update: AppUpdateInfo | null;
}

const [updateState, setUpdateState] = createStore<AppUpdateState>({
  checking: false,
  update: null,
});

export { updateState };

export function setAppUpdateForTests(update: AppUpdateInfo | null): void {
  setUpdateState({ checking: false, update });
}

interface GithubReleasePayload {
  tag_name?: unknown;
  name?: unknown;
  html_url?: unknown;
  draft?: unknown;
  prerelease?: unknown;
}

function parseVersion(value: string): [number, number, number] | null {
  const match = value.trim().match(/^v?(\d+)\.(\d+)\.(\d+)/i);
  if (!match) return null;
  return [Number(match[1]), Number(match[2]), Number(match[3])];
}

function versionCore(value: string): string {
  const parsed = parseVersion(value);
  return parsed ? parsed.join(".") : value.trim().replace(/^v/i, "");
}

export function compareVersions(a: string, b: string): -1 | 0 | 1 {
  const left = parseVersion(a);
  const right = parseVersion(b);
  if (!left || !right) return 0;

  for (let i = 0; i < 3; i += 1) {
    if (left[i] > right[i]) return 1;
    if (left[i] < right[i]) return -1;
  }
  return 0;
}

export function normalizeRelease(payload: GithubReleasePayload): NormalizedRelease | null {
  if (payload.draft === true) return null;
  if (typeof payload.tag_name !== "string" || payload.tag_name.trim().length === 0) return null;
  if (typeof payload.html_url !== "string" || payload.html_url.trim().length === 0) return null;

  const version = versionCore(payload.tag_name);
  if (!parseVersion(version)) return null;

  return {
    tagName: payload.tag_name,
    version,
    name: typeof payload.name === "string" && payload.name.trim() ? payload.name : payload.tag_name,
    releaseUrl: payload.html_url,
    prerelease: payload.prerelease === true,
  };
}

export function getDismissedUpdateTag(): string | null {
  try {
    return localStorage.getItem(DISMISSED_UPDATE_TAG_KEY);
  } catch {
    return null;
  }
}

export function dismissUpdate(tagName: string): void {
  try {
    localStorage.setItem(DISMISSED_UPDATE_TAG_KEY, tagName);
  } catch {
    // Ignore persistence failures; the modal can still be closed for this session.
  }
}

export function shouldDismissUpdatePrompt(result: string): boolean {
  return result === "later";
}

export function clearDismissedUpdateForTests(): void {
  try {
    localStorage.removeItem(DISMISSED_UPDATE_TAG_KEY);
  } catch {
    // Test helper only.
  }
}

function errorMessage(err: unknown): string {
  if (err instanceof Error && err.message) return err.message;
  if (typeof err === "string") return err;
  return "Unable to check for updates";
}

export async function checkForAppUpdate(
  currentVersion = import.meta.env.VITE_APP_VERSION
): Promise<AppUpdateInfo> {
  try {
    const response = await fetch(LATEST_RELEASE_URL, {
      headers: {
        Accept: "application/vnd.github+json",
      },
    });

    if (!response.ok) {
      return {
        available: false,
        dismissed: false,
        currentVersion,
        error: `GitHub release check failed: HTTP ${response.status}`,
      };
    }

    const release = normalizeRelease(await response.json());
    if (!release || compareVersions(release.version, currentVersion) <= 0) {
      return { available: false, dismissed: false, currentVersion };
    }

    const dismissed = getDismissedUpdateTag() === release.tagName;
    return {
      available: true,
      dismissed,
      currentVersion,
      latestVersion: release.version,
      tagName: release.tagName,
      releaseName: release.name,
      releaseUrl: release.releaseUrl,
    };
  } catch (err) {
    return {
      available: false,
      dismissed: false,
      currentVersion,
      error: errorMessage(err),
    };
  }
}

export async function refreshAppUpdate(): Promise<AppUpdateInfo> {
  setUpdateState("checking", true);
  const update = await checkForAppUpdate();
  setUpdateState({ checking: false, update });
  return update;
}

export async function openUpdateRelease(update: Pick<AppUpdateInfo, "releaseUrl">): Promise<void> {
  await openUrl(update.releaseUrl ?? RELEASES_PAGE_URL);
}
