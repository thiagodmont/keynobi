# Keynobi

**One fast macOS window for the Android build-and-debug loop, for you and your AI agent.**

## The Problem

Shipping an Android app means constant switching: a terminal for Gradle, Android Studio's Logcat tab, `adb` commands, the emulator manager, and a pile of one-off scripts. AI coding agents have it worse. They can't see your build output, your logs, or your device, so they guess, or you paste screenshots for them.

## What Keynobi Does

Keynobi sits next to Android Studio (or any editor) and handles everything around the code:

- **Build and run.** Build any variant, see errors as a clickable list, and install and launch on your device with `Cmd+R`.
- **Logs.** Stream logcat with fast filters, saved searches, and crash highlighting, without losing context in long sessions.
- **Screens.** Capture the UI hierarchy of any app, including Jetpack Compose, to see what is on screen and why.
- **Devices.** See connected phones and emulators; create, start, stop, and wipe emulators.
- **Setup checks.** Find out quickly whether the SDK, ADB, JDK, and emulator are ready.

## Why It Matters

- **Less tab-hopping.** Build, device, and logs live in one window, so the loop from "change" to "see the result" is shorter.
- **AI agents work with real data.** Keynobi's MCP server gives Claude Code, Codex, and other agents the same abilities: run Gradle, read build errors and crashes, and operate the device. They work from your actual project and device state, not from pasted text.
- **Private by default.** Everything runs on your Mac. Crash reporting is off unless you turn it on.

## How the AI Connection Works

You register Keynobi with your agent once. The agent then starts a small Keynobi process in the background for the project you pass with `--project`, or else the Android project it runs in. When Keynobi is open on that project, the agent works through the app: its builds appear in the Build tab, and it shares the app's devices and logcat. Otherwise it runs on its own, falling back to the last project you opened in Keynobi, and the Keynobi window does not need to be open.

## Who It's For

Android developers on **macOS** working with **Gradle** projects (Kotlin or Java), especially those who pair with AI coding agents. Keynobi is in beta.

**Try it:** [download the latest release](https://github.com/thiagodmont/keynobi/releases/latest).
