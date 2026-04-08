# Keynobi

**A focused macOS companion for Android development** — build output, logcat, and devices in one place, built to sit next to Android Studio and Claude Code.

## Vision

Shipping Android apps still means juggling Gradle logs, `adb`, emulators, and crash signals. Keynobi exists to give you a **single, fast surface** for that loop while you keep editing in the IDE and automating with AI. The goal is less tab-hopping and a tighter feedback cycle between **build → device → logs → fix**.

## What it does

- **Build** — Stream Gradle output, surface structured errors and warnings, run variants, and drive install/launch against the device you pick.
- **Logcat** — Live device logs with filtering, crash highlighting, and enough buffer to keep context during long sessions.
- **Devices** — See connected hardware and AVDs; start/stop emulators from the app.
- **Projects** — Keep a registry of Android projects and switch without losing your place.
- **MCP** — Expose builds, logcat, devices, and health checks to [Claude Code](https://claude.com/claude-code) (or any MCP client) so agents can run Gradle, read logs, and operate devices **using the same project you have open in Keynobi**.

## Why developers care

- **One window for the run/debug story** instead of stitching together terminal panes, Logcat tabs, and one-off scripts.
- **Agent-ready**: MCP turns “ask the AI to fix the build” into something grounded in **your** tree, **your** Gradle output, and **your** device state — not pasted screenshots.
- **Privacy-minded defaults** — Anonymous crash reporting is opt-in; you stay in control of what leaves the machine.

## Fit

**macOS** today. **Kotlin + Gradle** Android projects (Gradle root detection and project-aware workflows). If that’s your stack and you want a tighter companion around Studio and Claude Code, Keynobi is for you.
