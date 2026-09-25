# `dumpsys activity exit-info` fixtures

Output of `adb shell dumpsys activity exit-info <package>`, used by the parser tests in
`src/services/app_exit_info.rs` and the MCP test in `tests/mcp_headless.rs`.

These files are **reconstructed, not captured from devices**. They follow the dump code in the
Android framework (`AppExitInfoTracker.dumpHistoryProcessExitInfo`,
`AppExitInfoContainer.dumpLocked`, `ApplicationExitInfo.dump`, and
`DebugUtils.sizeValueToString` for `pss`/`rss`), with the reason codes each release added.
Replace a file with a real capture when one is available, sanitized the same way as
`build_output/`.

| File | Android | What it covers |
|------|---------|----------------|
| `android11.txt` | 11 (API 30) | Crash, ANR with a long description and a trace path, low memory with zero sizes, exit self, `KB` sizes |
| `android12.txt` | 12 (API 31–32) | User request, a signaled secondary process, a system kill with a sub-reason |
| `android13.txt` | 13 (API 33) | Native crash with its signal, dependency died, excessive CPU whose description contains `key=value` text |
| `android14.txt` | 14 (API 34) | Freezer, `GB` sizes, a second user's uid in its own section (so records are not in time order across sections) |
| `android15.txt` | 15 (API 35) | Package updated |
| `odd_fields.txt` | Any | Unknown reason code and keys, a description over two lines, a locale-formatted time, a record with one field, unparseable values |
| `below_api30.txt` | 10 and older | What `dumpsys activity` prints for a command it does not know |
| `no_records.txt` | Any | The header only: no exits recorded for the package |
