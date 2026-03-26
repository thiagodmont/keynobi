/**
 * Strip ANSI escape codes from a string.
 *
 * We strip rather than convert to HTML because the LogViewer already
 * handles colour styling via log level. ANSI colour codes from Gradle
 * would otherwise render as literal ESC characters.
 */
export function stripAnsi(str: string): string {
  // ESC [ ... m and other CSI sequences, plus simple ESC sequences.
  // eslint-disable-next-line no-control-regex
  return str.replace(/\x1b\[[0-9;]*[mGKHFABCDSTJsu]|\x1b[^[]/g, "");
}
