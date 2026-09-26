// Generated from the Rust IPC types by `npm run generate:ipc-fixtures`. Do not edit.
import type {
  AppError,
  AppExitReasons,
  AppSettings,
  AvailableSystemImage,
  AvdInfo,
  BuildCompleteEvent,
  BuildError,
  BuildLine,
  BuildLinesEvent,
  BuildRecord,
  BuildStartedEvent,
  BuildStatus,
  BuiltApk,
  Device,
  DeviceDefinition,
  DeviceListChangedEvent,
  InstalledBuild,
  LaunchResult,
  LaunchState,
  LaunchTiming,
  LogStats,
  MappingMatch,
  MappingSnapshot,
  McpActivityEntry,
  McpAttachedSession,
  McpServerStatus,
  McpSetupStatus,
  MonitorStats,
  ProcessedEntry,
  ProjectAppInfo,
  ProjectEntry,
  RetraceOutcome,
  RetraceStatus,
  SdkDownloadProgress,
  SystemHealthReport,
  SystemImageInfo,
  UiHierarchySnapshot,
  VariantList,
} from "@/bindings";
import type { Wire } from "./wire";

/** Serialized samples of each IPC type, as the backend sends them. */
export const typeFixtures = {
  AppError: [
    {
      "kind": "notFound",
      "message": "settings.json"
    },
    {
      "kind": "permissionDenied",
      "message": "/p"
    },
    {
      "kind": "invalidInput",
      "message": "bad task name"
    },
    {
      "kind": "io",
      "message": "'/p': disk full"
    },
    {
      "kind": "processFailed",
      "message": "adb exited with 1"
    },
    {
      "kind": "settingsError",
      "message": "unreadable"
    },
    {
      "kind": "mcpError",
      "message": "not listening"
    },
    {
      "kind": "other",
      "message": "unexpected"
    }
  ] satisfies Wire<AppError>[],
  ProjectEntry: [
    {
      "gradleRoot": "/p",
      "id": "3f2a",
      "lastBuildVariant": "debug",
      "lastDevice": "emulator-5554",
      "lastOpened": "2026-04-23T10:00:00Z",
      "name": "Sample",
      "path": "/p",
      "pinned": true,
      "trusted": true
    },
    {
      "gradleRoot": null,
      "id": "9c1d",
      "lastBuildVariant": null,
      "lastDevice": null,
      "lastOpened": "2026-04-23T10:00:00Z",
      "name": "Other",
      "path": "/q",
      "pinned": false,
      "trusted": null
    }
  ] satisfies Wire<ProjectEntry>[],
  ProjectAppInfo: [
    {
      "applicationId": "com.example.app",
      "versionCode": 42,
      "versionCodeUnavailable": null,
      "versionName": "1.2.3",
      "versionNameUnavailable": null
    },
    {
      "applicationId": null,
      "versionCode": null,
      "versionCodeUnavailable": "versionCode in app/build.gradle.kts (line 7) is set by `libs.versions.code.get().toInt()`",
      "versionName": null,
      "versionNameUnavailable": "No versionName assignment found in app/build.gradle.kts"
    }
  ] satisfies Wire<ProjectAppInfo>[],
  AppSettings: [
    {
      "advanced": {
        "diagnosticsPullDelayMs": 1000,
        "hoverDelayMs": 500,
        "logMaxSizeMb": 500,
        "logRetentionDays": 7,
        "lspDidChangeDebounceMs": 300,
        "lspMaxMessageSizeMb": 64,
        "navigationHistoryDepth": 50,
        "recentFilesLimit": 20,
        "treeSitterCacheSize": 50,
        "watcherDebounceMs": 200
      },
      "android": {
        "sdkPath": "/sdk"
      },
      "appearance": {
        "uiFontSize": 12
      },
      "build": {
        "autoInstallOnBuild": true,
        "autoScrollBuildLog": true,
        "buildLogMaxFolderMb": 100,
        "buildLogRetentionDays": 7
      },
      "java": {
        "home": "/jdk"
      },
      "lastActiveProject": "/p",
      "logcat": {
        "autoScrollToEnd": true,
        "autoStart": true,
        "maxUiLines": 20000,
        "outputFontSize": 11,
        "ringMaxEntries": 50000
      },
      "lsp": {
        "logLevel": "INFO",
        "requestTimeoutSec": 30
      },
      "mcp": {
        "allowUnrestrictedGradle": false,
        "buildLogDefaultLines": 200,
        "buildTimeoutSec": 600,
        "logcatDefaultCount": 200
      },
      "onboardingCompleted": false,
      "recentProjects": [
        {
          "gradleRoot": "/p",
          "id": "3f2a",
          "lastBuildVariant": "debug",
          "lastDevice": "emulator-5554",
          "lastOpened": "2026-04-23T10:00:00Z",
          "name": "Sample",
          "path": "/p",
          "pinned": true,
          "trusted": true
        },
        {
          "gradleRoot": null,
          "id": "9c1d",
          "lastBuildVariant": null,
          "lastDevice": null,
          "lastOpened": "2026-04-23T10:00:00Z",
          "name": "Other",
          "path": "/q",
          "pinned": false,
          "trusted": null
        }
      ],
      "search": {
        "contextLines": 2,
        "maxFiles": 500,
        "maxResults": 10000
      },
      "telemetry": {
        "enabled": false
      }
    },
    {
      "advanced": {
        "diagnosticsPullDelayMs": 1000,
        "hoverDelayMs": 500,
        "logMaxSizeMb": 500,
        "logRetentionDays": 7,
        "lspDidChangeDebounceMs": 300,
        "lspMaxMessageSizeMb": 64,
        "navigationHistoryDepth": 50,
        "recentFilesLimit": 20,
        "treeSitterCacheSize": 50,
        "watcherDebounceMs": 200
      },
      "android": {
        "sdkPath": null
      },
      "appearance": {
        "uiFontSize": 12
      },
      "build": {
        "autoInstallOnBuild": true,
        "autoScrollBuildLog": true,
        "buildLogMaxFolderMb": 100,
        "buildLogRetentionDays": 7
      },
      "java": {
        "home": null
      },
      "lastActiveProject": null,
      "logcat": {
        "autoScrollToEnd": true,
        "autoStart": true,
        "maxUiLines": 20000,
        "outputFontSize": 11,
        "ringMaxEntries": 50000
      },
      "lsp": {
        "logLevel": "INFO",
        "requestTimeoutSec": 30
      },
      "mcp": {
        "allowUnrestrictedGradle": false,
        "buildLogDefaultLines": 200,
        "buildTimeoutSec": 600,
        "logcatDefaultCount": 200
      },
      "onboardingCompleted": false,
      "recentProjects": [],
      "search": {
        "contextLines": 2,
        "maxFiles": 500,
        "maxResults": 10000
      },
      "telemetry": {
        "enabled": false
      }
    }
  ] satisfies Wire<AppSettings>[],
  SystemHealthReport: [
    {
      "adbFound": true,
      "adbVersion": "Android Debug Bridge version 1.0.41",
      "androidSdkValid": true,
      "appLocationProblem": null,
      "emulatorFound": true,
      "gradleWrapperFound": true,
      "javaBinUsed": "/jdk/bin/java",
      "javaExecutableFound": true,
      "javaHome": "/jdk",
      "javaMajorVersion": 17,
      "javaSource": "androidStudio",
      "javaVersion": "openjdk version \"17.0.9\"",
      "lspSystemDirOk": true,
      "retraceVersion": "22.0",
      "studioCommandFound": false
    },
    {
      "adbFound": false,
      "adbVersion": null,
      "androidSdkValid": false,
      "appLocationProblem": "Keynobi is running from a disk image.",
      "emulatorFound": false,
      "gradleWrapperFound": false,
      "javaBinUsed": "java",
      "javaExecutableFound": false,
      "javaHome": null,
      "javaMajorVersion": null,
      "javaSource": null,
      "javaVersion": null,
      "lspSystemDirOk": true,
      "retraceVersion": null,
      "studioCommandFound": false
    }
  ] satisfies Wire<SystemHealthReport>[],
  BuildStatus: [
    {
      "state": "idle"
    },
    {
      "started_at": "2026-04-23T10:00:00Z",
      "state": "running",
      "task": "assembleDebug"
    },
    {
      "durationMs": 4200,
      "errorCount": 0,
      "state": "success",
      "success": true,
      "warningCount": 2
    },
    {
      "durationMs": 4200,
      "errorCount": 1,
      "state": "failed",
      "success": false,
      "warningCount": 2
    },
    {
      "state": "cancelled"
    }
  ] satisfies Wire<BuildStatus>[],
  BuildLine: [
    {
      "col": 5,
      "content": "e: file:///p/app/src/main/java/Main.kt:10:5 Unresolved reference: foo",
      "file": "/p/app/src/main/java/Main.kt",
      "kind": "error",
      "line": 10
    },
    {
      "col": null,
      "content": "> Task :app:assembleDebug",
      "file": null,
      "kind": "output",
      "line": null
    }
  ] satisfies Wire<BuildLine>[],
  BuildError: [
    {
      "col": 5,
      "file": "/p/app/src/main/java/Main.kt",
      "line": 10,
      "message": "Unresolved reference: foo",
      "severity": "error"
    },
    {
      "col": null,
      "file": null,
      "line": null,
      "message": "Could not resolve com.example:lib:1.0",
      "severity": "warning"
    }
  ] satisfies Wire<BuildError>[],
  BuildRecord: [
    {
      "apks": [],
      "cancelledBy": {
        "kind": "app"
      },
      "errors": [],
      "id": 7,
      "launch": null,
      "mappings": [],
      "origin": {
        "kind": "app"
      },
      "projectRoot": "/p",
      "startedAt": "2026-04-23T10:00:00Z",
      "status": {
        "state": "cancelled"
      },
      "task": "assembleDebug"
    },
    {
      "apks": [],
      "cancelledBy": {
        "kind": "appQuit"
      },
      "errors": [],
      "id": 8,
      "launch": null,
      "mappings": [],
      "origin": {
        "kind": "appQuit"
      },
      "projectRoot": "/p",
      "startedAt": "2026-04-23T10:00:00Z",
      "status": {
        "state": "cancelled"
      },
      "task": "assembleDebug"
    },
    {
      "apks": [],
      "cancelledBy": {
        "clientName": "Claude Code",
        "kind": "agent",
        "sessionId": 2,
        "standalone": false
      },
      "errors": [],
      "id": 9,
      "launch": null,
      "mappings": [],
      "origin": {
        "clientName": "Claude Code",
        "kind": "agent",
        "sessionId": 2,
        "standalone": false
      },
      "projectRoot": "/p",
      "startedAt": "2026-04-23T10:00:00Z",
      "status": {
        "state": "cancelled"
      },
      "task": "assembleDebug"
    },
    {
      "apks": [],
      "cancelledBy": {
        "clientName": null,
        "kind": "agent",
        "sessionId": null,
        "standalone": true
      },
      "errors": [],
      "id": 10,
      "launch": null,
      "mappings": [],
      "origin": {
        "clientName": null,
        "kind": "agent",
        "sessionId": null,
        "standalone": true
      },
      "projectRoot": "/p",
      "startedAt": "2026-04-23T10:00:00Z",
      "status": {
        "state": "cancelled"
      },
      "task": "assembleDebug"
    },
    {
      "apks": [],
      "cancelledBy": null,
      "errors": [
        {
          "col": 5,
          "file": "/p/app/src/main/java/Main.kt",
          "line": 10,
          "message": "Unresolved reference: foo",
          "severity": "error"
        },
        {
          "col": null,
          "file": null,
          "line": null,
          "message": "Could not resolve com.example:lib:1.0",
          "severity": "warning"
        }
      ],
      "id": 11,
      "launch": null,
      "mappings": [],
      "origin": null,
      "projectRoot": null,
      "startedAt": "2026-04-23T10:00:00Z",
      "status": {
        "durationMs": 4200,
        "errorCount": 1,
        "state": "failed",
        "success": false,
        "warningCount": 2
      },
      "task": "assembleDebug"
    },
    {
      "apks": [
        {
          "applicationId": "com.example.app",
          "bytes": 12582912,
          "module": ":app",
          "path": "app/build/outputs/apk/release/app-release.apk",
          "sha256": "a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1",
          "variant": "release",
          "versionCode": 42
        },
        {
          "applicationId": null,
          "bytes": 4096,
          "module": ":wear",
          "path": "wear/build/outputs/apk/paid/release/wear-paid-release.apk",
          "sha256": "b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2",
          "variant": "paidRelease",
          "versionCode": null
        }
      ],
      "cancelledBy": null,
      "errors": [],
      "id": 20,
      "launch": {
        "avdName": "Pixel_7_API_34",
        "launchState": "cold",
        "measuredAt": "2026-04-23T10:00:00Z",
        "model": "sdk_gphone64_arm64",
        "serial": "emulator-5554",
        "totalMs": 812,
        "waitMs": 815
      },
      "mappings": [
        {
          "bytes": 48213771,
          "module": ":app",
          "pgMapId": "6b1c2f0",
          "sha256": "6b1c2f0a6b1c2f0a6b1c2f0a6b1c2f0a6b1c2f0a6b1c2f0a6b1c2f0a6b1c2f0a",
          "variant": "release"
        },
        {
          "bytes": 1024,
          "module": ":wear",
          "pgMapId": null,
          "sha256": "0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f",
          "variant": "paidRelease"
        }
      ],
      "origin": {
        "kind": "app"
      },
      "projectRoot": "/p",
      "startedAt": "2026-04-23T10:00:00Z",
      "status": {
        "durationMs": 4200,
        "errorCount": 0,
        "state": "success",
        "success": true,
        "warningCount": 2
      },
      "task": "assembleDebug"
    },
    {
      "apks": [
        {
          "applicationId": "com.example.app",
          "bytes": 12582912,
          "module": ":app",
          "path": "app/build/outputs/apk/release/app-release.apk",
          "sha256": "a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1",
          "variant": "release",
          "versionCode": 42
        },
        {
          "applicationId": null,
          "bytes": 4096,
          "module": ":wear",
          "path": "wear/build/outputs/apk/paid/release/wear-paid-release.apk",
          "sha256": "b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2",
          "variant": "paidRelease",
          "versionCode": null
        }
      ],
      "cancelledBy": null,
      "errors": [],
      "id": 20,
      "launch": {
        "avdName": null,
        "launchState": null,
        "measuredAt": "2026-04-23T10:00:00Z",
        "model": null,
        "serial": "28151FDH2000Q4",
        "totalMs": 640,
        "waitMs": null
      },
      "mappings": [
        {
          "bytes": 48213771,
          "module": ":app",
          "pgMapId": "6b1c2f0",
          "sha256": "6b1c2f0a6b1c2f0a6b1c2f0a6b1c2f0a6b1c2f0a6b1c2f0a6b1c2f0a6b1c2f0a",
          "variant": "release"
        },
        {
          "bytes": 1024,
          "module": ":wear",
          "pgMapId": null,
          "sha256": "0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f",
          "variant": "paidRelease"
        }
      ],
      "origin": {
        "kind": "app"
      },
      "projectRoot": "/p",
      "startedAt": "2026-04-23T10:00:00Z",
      "status": {
        "durationMs": 4200,
        "errorCount": 0,
        "state": "success",
        "success": true,
        "warningCount": 2
      },
      "task": "assembleDebug"
    }
  ] satisfies Wire<BuildRecord>[],
  MappingSnapshot: [
    {
      "bytes": 48213771,
      "module": ":app",
      "pgMapId": "6b1c2f0",
      "sha256": "6b1c2f0a6b1c2f0a6b1c2f0a6b1c2f0a6b1c2f0a6b1c2f0a6b1c2f0a6b1c2f0a",
      "variant": "release"
    },
    {
      "bytes": 1024,
      "module": ":wear",
      "pgMapId": null,
      "sha256": "0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f0f",
      "variant": "paidRelease"
    }
  ] satisfies Wire<MappingSnapshot>[],
  BuiltApk: [
    {
      "applicationId": "com.example.app",
      "bytes": 12582912,
      "module": ":app",
      "path": "app/build/outputs/apk/release/app-release.apk",
      "sha256": "a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1",
      "variant": "release",
      "versionCode": 42
    },
    {
      "applicationId": null,
      "bytes": 4096,
      "module": ":wear",
      "path": "wear/build/outputs/apk/paid/release/wear-paid-release.apk",
      "sha256": "b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2b2",
      "variant": "paidRelease",
      "versionCode": null
    }
  ] satisfies Wire<BuiltApk>[],
  InstalledBuild: [
    {
      "apkSha256": "a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1a1",
      "avdName": "Pixel_7",
      "buildId": 20,
      "installedAt": "2026-04-23T10:00:00Z",
      "mappings": [
        {
          "bytes": 48213771,
          "module": ":app",
          "pgMapId": "6b1c2f0",
          "sha256": "6b1c2f0a6b1c2f0a6b1c2f0a6b1c2f0a6b1c2f0a6b1c2f0a6b1c2f0a6b1c2f0a",
          "variant": "release"
        }
      ],
      "model": "sdk_gphone64_arm64",
      "package": "com.example.app",
      "serial": "emulator-5554",
      "versionCode": 42
    },
    {
      "apkSha256": "c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3c3",
      "avdName": null,
      "buildId": null,
      "installedAt": "2026-04-23T10:00:00Z",
      "mappings": [],
      "model": null,
      "package": "com.example.app.debug",
      "serial": "R5CT1234ABC",
      "versionCode": null
    }
  ] satisfies Wire<InstalledBuild>[],
  LaunchState: [
    "cold",
    "warm",
    "hot",
    "relaunch"
  ] satisfies Wire<LaunchState>[],
  LaunchTiming: [
    {
      "avdName": "Pixel_7_API_34",
      "launchState": "cold",
      "measuredAt": "2026-04-23T10:00:00Z",
      "model": "sdk_gphone64_arm64",
      "serial": "emulator-5554",
      "totalMs": 812,
      "waitMs": 815
    },
    {
      "avdName": null,
      "launchState": null,
      "measuredAt": "2026-04-23T10:00:00Z",
      "model": null,
      "serial": "28151FDH2000Q4",
      "totalMs": 640,
      "waitMs": null
    }
  ] satisfies Wire<LaunchTiming>[],
  LaunchResult: [
    {
      "output": "am start OK: Status: ok",
      "timing": {
        "avdName": "Pixel_7_API_34",
        "launchState": "cold",
        "measuredAt": "2026-04-23T10:00:00Z",
        "model": "sdk_gphone64_arm64",
        "serial": "emulator-5554",
        "totalMs": 812,
        "waitMs": 815
      }
    },
    {
      "output": "monkey OK: Events injected: 1",
      "timing": null
    }
  ] satisfies Wire<LaunchResult>[],
  VariantList: [
    {
      "active": "freeDebug",
      "defaultVariant": "freeDebug",
      "variants": [
        {
          "assembleTask": "assembleFreeDebug",
          "buildType": "debug",
          "flavors": [
            "free"
          ],
          "installTask": "installFreeDebug",
          "name": "freeDebug"
        }
      ]
    },
    {
      "active": null,
      "defaultVariant": null,
      "variants": []
    }
  ] satisfies Wire<VariantList>[],
  BuildStartedEvent: [
    {
      "origin": {
        "kind": "app"
      },
      "projectRoot": "/p",
      "runId": 9001,
      "startedAt": "2026-04-23T10:00:00Z",
      "task": "assembleDebug"
    },
    {
      "origin": {
        "kind": "appQuit"
      },
      "projectRoot": null,
      "runId": 9002,
      "startedAt": "2026-04-23T10:00:00Z",
      "task": "assembleDebug"
    },
    {
      "origin": {
        "clientName": "Claude Code",
        "kind": "agent",
        "sessionId": 2,
        "standalone": false
      },
      "projectRoot": "/p",
      "runId": 9003,
      "startedAt": "2026-04-23T10:00:00Z",
      "task": "assembleDebug"
    },
    {
      "origin": {
        "clientName": null,
        "kind": "agent",
        "sessionId": null,
        "standalone": true
      },
      "projectRoot": null,
      "runId": 9004,
      "startedAt": "2026-04-23T10:00:00Z",
      "task": "assembleDebug"
    }
  ] satisfies Wire<BuildStartedEvent>[],
  BuildLinesEvent: [
    {
      "lines": [
        {
          "col": 5,
          "content": "e: file:///p/app/src/main/java/Main.kt:10:5 Unresolved reference: foo",
          "file": "/p/app/src/main/java/Main.kt",
          "kind": "error",
          "line": 10
        },
        {
          "col": null,
          "content": "> Task :app:assembleDebug",
          "file": null,
          "kind": "output",
          "line": null
        }
      ],
      "runId": 9001
    }
  ] satisfies Wire<BuildLinesEvent>[],
  BuildCompleteEvent: [
    {
      "cancelled": true,
      "cancelledBy": {
        "kind": "app"
      },
      "durationMs": 4200,
      "errorCount": 0,
      "origin": {
        "kind": "app"
      },
      "recordId": 7,
      "runId": 9001,
      "success": false,
      "task": "assembleDebug",
      "warningCount": 1
    },
    {
      "cancelled": true,
      "cancelledBy": {
        "kind": "appQuit"
      },
      "durationMs": 4200,
      "errorCount": 0,
      "origin": {
        "kind": "appQuit"
      },
      "recordId": 8,
      "runId": 9002,
      "success": false,
      "task": "assembleDebug",
      "warningCount": 1
    },
    {
      "cancelled": true,
      "cancelledBy": {
        "clientName": "Claude Code",
        "kind": "agent",
        "sessionId": 2,
        "standalone": false
      },
      "durationMs": 4200,
      "errorCount": 0,
      "origin": {
        "clientName": "Claude Code",
        "kind": "agent",
        "sessionId": 2,
        "standalone": false
      },
      "recordId": 9,
      "runId": 9003,
      "success": false,
      "task": "assembleDebug",
      "warningCount": 1
    },
    {
      "cancelled": true,
      "cancelledBy": {
        "clientName": null,
        "kind": "agent",
        "sessionId": null,
        "standalone": true
      },
      "durationMs": 4200,
      "errorCount": 0,
      "origin": {
        "clientName": null,
        "kind": "agent",
        "sessionId": null,
        "standalone": true
      },
      "recordId": 10,
      "runId": 9004,
      "success": false,
      "task": "assembleDebug",
      "warningCount": 1
    },
    {
      "cancelled": false,
      "cancelledBy": null,
      "durationMs": 4200,
      "errorCount": 0,
      "origin": null,
      "recordId": 11,
      "runId": 9005,
      "success": true,
      "task": "assembleDebug",
      "warningCount": 1
    }
  ] satisfies Wire<BuildCompleteEvent>[],
  Device: [
    {
      "androidVersion": "14",
      "apiLevel": 34,
      "avdName": "Pixel_7_API_34",
      "connectionState": "online",
      "deviceKind": "emulator",
      "model": "sdk_gphone64_arm64",
      "name": "Pixel 7",
      "serial": "emulator-5554"
    },
    {
      "androidVersion": "15",
      "apiLevel": 35,
      "connectionState": "online",
      "deviceKind": "physical",
      "model": "Pixel 7",
      "name": "Pixel 7",
      "serial": "28151FDH2000Q4"
    },
    {
      "androidVersion": null,
      "apiLevel": null,
      "connectionState": "unauthorized",
      "deviceKind": "physical",
      "model": null,
      "name": "ZX1G22ABCD",
      "serial": "ZX1G22ABCD"
    }
  ] satisfies Wire<Device>[],
  RetraceOutcome: [
    {
      "buildId": 20,
      "device": "Pixel_7",
      "mapping": {
        "bytes": 48213771,
        "module": ":app",
        "pgMapId": "6b1c2f0",
        "sha256": "6b1c2f0a6b1c2f0a6b1c2f0a6b1c2f0a6b1c2f0a6b1c2f0a6b1c2f0a6b1c2f0a",
        "variant": "release"
      },
      "matchedBy": "installRecord",
      "package": "com.example.app",
      "reason": null,
      "status": "retraced",
      "summary": "Deobfuscated with the R8 mapping of build #20 (:app release, map id 6b1c2f0), matched by Keynobi's install on Pixel_7 at 2026-04-23T10:00:00Z and confirmed by the device (versionCode 42, last updated 2026-04-23 10:00:00).",
      "trace": "java.lang.RuntimeException: boom\n\tat com.example.app.MainActivity.onCreate(MainActivity.kt:24)\n"
    },
    {
      "buildId": null,
      "device": null,
      "mapping": null,
      "matchedBy": null,
      "package": null,
      "reason": "logcat did not attribute the crash to a package, so its build is unknown",
      "status": "refused",
      "summary": "Not deobfuscated: logcat did not attribute the crash to a package, so its build is unknown.",
      "trace": "java.lang.RuntimeException: boom\n\tat a.a.onCreate(SourceFile:1)\n"
    }
  ] satisfies Wire<RetraceOutcome>[],
  RetraceStatus: [
    "retraced",
    "unavailable",
    "refused",
    "failed"
  ] satisfies Wire<RetraceStatus>[],
  MappingMatch: [
    "installRecord"
  ] satisfies Wire<MappingMatch>[],
  DeviceListChangedEvent: [
    {
      "devices": [
        {
          "androidVersion": "14",
          "apiLevel": 34,
          "avdName": "Pixel_7_API_34",
          "connectionState": "online",
          "deviceKind": "emulator",
          "model": "sdk_gphone64_arm64",
          "name": "Pixel 7",
          "serial": "emulator-5554"
        },
        {
          "androidVersion": "15",
          "apiLevel": 35,
          "connectionState": "online",
          "deviceKind": "physical",
          "model": "Pixel 7",
          "name": "Pixel 7",
          "serial": "28151FDH2000Q4"
        },
        {
          "androidVersion": null,
          "apiLevel": null,
          "connectionState": "unauthorized",
          "deviceKind": "physical",
          "model": null,
          "name": "ZX1G22ABCD",
          "serial": "ZX1G22ABCD"
        }
      ]
    }
  ] satisfies Wire<DeviceListChangedEvent>[],
  AppExitReasons: [
    {
      "apiLevel": 34,
      "message": null,
      "package": "com.example.app.debug",
      "records": [
        {
          "description": "crash",
          "importance": 100,
          "importanceName": "foreground",
          "pid": 31020,
          "processName": "com.example.app.debug",
          "pssKb": 56320,
          "reason": "crash",
          "reasonCode": 4,
          "reasonLabel": "APP CRASH(EXCEPTION)",
          "rssKb": 130048,
          "status": 0,
          "subReason": "UNKNOWN",
          "subReasonCode": 0,
          "timestamp": "2024-01-09 08:12:44.310",
          "timestampLocal": "2024-01-09T08:12:44.310"
        },
        {
          "description": null,
          "importance": null,
          "importanceName": null,
          "pid": null,
          "processName": null,
          "pssKb": null,
          "reason": "unknown",
          "reasonCode": null,
          "reasonLabel": null,
          "rssKb": null,
          "status": null,
          "subReason": null,
          "subReasonCode": null,
          "timestamp": null,
          "timestampLocal": null
        }
      ],
      "serial": "emulator-5554",
      "supported": true,
      "totalRecords": 2
    },
    {
      "apiLevel": null,
      "message": "Process exit reasons need Android 11 (API 30) or later; emulator-5556 runs API 29.",
      "package": "com.example.app",
      "records": [],
      "serial": "emulator-5556",
      "supported": false,
      "totalRecords": 0
    }
  ] satisfies Wire<AppExitReasons>[],
  AvdInfo: [
    {
      "abi": "arm64-v8a",
      "apiLevel": 34,
      "displayName": "Pixel 7 API 34",
      "name": "Pixel_7_API_34",
      "path": "/home/.android/avd/Pixel_7_API_34.avd",
      "target": "android-34"
    },
    {
      "abi": null,
      "apiLevel": null,
      "displayName": "Broken",
      "name": "Broken",
      "path": "/home/.android/avd/Broken.avd",
      "target": null
    }
  ] satisfies Wire<AvdInfo>[],
  SystemImageInfo: [
    {
      "abi": "arm64-v8a",
      "apiLevel": 34,
      "displayName": "Android 14 (Google APIs) · arm64-v8a",
      "sdkId": "system-images;android-34;google_apis;arm64-v8a",
      "variant": "google_apis"
    }
  ] satisfies Wire<SystemImageInfo>[],
  DeviceDefinition: [
    {
      "id": "pixel_7",
      "manufacturer": "Google",
      "name": "Pixel 7"
    }
  ] satisfies Wire<DeviceDefinition>[],
  AvailableSystemImage: [
    {
      "abi": "arm64-v8a",
      "apiLevel": 35,
      "displayName": "Android 15 (Google APIs) · arm64-v8a",
      "installed": false,
      "sdkId": "system-images;android-35;google_apis;arm64-v8a",
      "variant": "google_apis"
    }
  ] satisfies Wire<AvailableSystemImage>[],
  SdkDownloadProgress: [
    {
      "done": false,
      "error": false,
      "message": "Downloading...",
      "percent": 40
    },
    {
      "done": true,
      "error": false,
      "message": "Installing...",
      "percent": null
    }
  ] satisfies Wire<SdkDownloadProgress>[],
  UiHierarchySnapshot: [
    {
      "capturedAt": "2026-04-23T10:00:00Z",
      "commandLog": [
        "adb -s emulator-5554 shell uiautomator dump"
      ],
      "foregroundActivity": "com.example.app/.MainActivity",
      "interactiveCount": 1,
      "layoutContext": {
        "displayExcerpt": "mBaseDisplayInfo=…",
        "windowExcerpt": "mCurrentFocus=Window{…}",
        "wmDensity": "Physical density: 420",
        "wmSize": "Physical size: 1080x2400"
      },
      "root": {
        "bounds": "[0,0][1080,2400]",
        "checkable": false,
        "checked": false,
        "children": [
          {
            "bounds": "[0,0][1080,2400]",
            "checkable": false,
            "checked": false,
            "children": [],
            "class": "android.widget.FrameLayout",
            "clickable": true,
            "contentDesc": "",
            "editable": false,
            "enabled": true,
            "focusable": false,
            "focused": false,
            "isComposeHeuristic": false,
            "longClickable": false,
            "package": "com.example.app",
            "password": false,
            "resourceId": "com.example.app:id/root",
            "scrollable": false,
            "selected": false,
            "text": "Sign in"
          }
        ],
        "class": "android.widget.FrameLayout",
        "clickable": true,
        "contentDesc": "",
        "editable": false,
        "enabled": true,
        "focusable": false,
        "focused": false,
        "isComposeHeuristic": false,
        "longClickable": false,
        "package": "com.example.app",
        "password": false,
        "resourceId": "com.example.app:id/root",
        "scrollable": false,
        "selected": false,
        "text": "Sign in"
      },
      "screenHash": "ab12",
      "screenshotB64": "iVBORw0KGgo=",
      "truncated": false,
      "warnings": [
        "compose semantics missing"
      ]
    },
    {
      "capturedAt": "2026-04-23T10:00:00Z",
      "commandLog": [],
      "foregroundActivity": null,
      "interactiveCount": 0,
      "layoutContext": {},
      "root": {
        "bounds": "[0,0][1080,2400]",
        "checkable": false,
        "checked": false,
        "children": [],
        "class": "android.widget.FrameLayout",
        "clickable": true,
        "contentDesc": "",
        "editable": false,
        "enabled": true,
        "focusable": false,
        "focused": false,
        "isComposeHeuristic": false,
        "longClickable": false,
        "package": "com.example.app",
        "password": false,
        "resourceId": "com.example.app:id/root",
        "scrollable": false,
        "selected": false,
        "text": "Sign in"
      },
      "screenHash": "cd34",
      "truncated": true,
      "warnings": []
    }
  ] satisfies Wire<UiHierarchySnapshot>[],
  ProcessedEntry: [
    {
      "category": "general",
      "crashGroupId": 41,
      "flags": 5,
      "id": 41,
      "isCrash": true,
      "jsonBody": "{\"ok\":false}",
      "kind": "normal",
      "level": "error",
      "message": "FATAL EXCEPTION: main",
      "package": "com.example.app",
      "pid": 1234,
      "tag": "AndroidRuntime",
      "tid": 1236,
      "timestamp": "2026-04-23T10:00:02.000Z"
    },
    {
      "category": "lifecycle",
      "crashGroupId": null,
      "flags": 0,
      "id": 42,
      "isCrash": false,
      "jsonBody": null,
      "kind": "processDied",
      "level": "info",
      "message": "Process com.example.app has died",
      "package": null,
      "pid": 1300,
      "tag": "ActivityManager",
      "tid": 1300,
      "timestamp": "2026-04-23T10:00:03.000Z"
    }
  ] satisfies Wire<ProcessedEntry>[],
  LogStats: [
    {
      "backlogLines": 5,
      "bufferEntryCount": 100,
      "bufferUsagePct": 0.5,
      "countsByLevel": [
        1,
        2,
        3,
        4,
        5,
        6,
        7
      ],
      "crashCount": 1,
      "droppedLines": 4,
      "jsonCount": 2,
      "packagesSeen": 3,
      "totalIngested": 120
    }
  ] satisfies Wire<LogStats>[],
  McpAttachedSession: [
    {
      "clientName": "Claude Code",
      "connectedAt": "2026-04-23T10:00:00Z",
      "id": 2,
      "pid": 4321,
      "project": "/p",
      "version": "0.1.29"
    },
    {
      "clientName": null,
      "connectedAt": "2026-04-23T10:00:00Z",
      "id": 3,
      "pid": null,
      "project": null,
      "version": "0.1.28"
    }
  ] satisfies Wire<McpAttachedSession>[],
  McpServerStatus: [
    {
      "appVersion": "0.1.29",
      "attached": [
        {
          "clientName": "Claude Code",
          "connectedAt": "2026-04-23T10:00:00Z",
          "id": 2,
          "pid": 4321,
          "project": "/p",
          "version": "0.1.29"
        },
        {
          "clientName": null,
          "connectedAt": "2026-04-23T10:00:00Z",
          "id": 3,
          "pid": null,
          "project": null,
          "version": "0.1.28"
        }
      ],
      "listening": true,
      "standalone": [
        {
          "pid": 5555,
          "project": "/p",
          "reason": "the Keynobi app is not running",
          "startedAt": "2026-04-23T10:00:00Z",
          "version": "0.1.29"
        },
        {
          "pid": 5556,
          "project": null,
          "reason": "the Keynobi app is not running",
          "startedAt": "2026-04-23T10:00:00Z",
          "version": null
        }
      ]
    }
  ] satisfies Wire<McpServerStatus>[],
  McpActivityEntry: [
    {
      "durationMs": 12,
      "kind": "tool_call",
      "name": "get_project_info",
      "status": "ok",
      "summary": "project open",
      "timestamp": "2026-04-23T10:00:00Z"
    },
    {
      "durationMs": null,
      "kind": "lifecycle",
      "name": "Server started",
      "status": "ok",
      "summary": null,
      "timestamp": "2026-04-23T10:00:00Z"
    }
  ] satisfies Wire<McpActivityEntry>[],
  McpSetupStatus: [
    {
      "claude": {
        "clientFound": true,
        "configuredCommand": "/Applications/Keynobi.app/Contents/MacOS/keynobi --mcp",
        "configuredScope": "user",
        "isConfigured": true,
        "setupCommand": "claude mcp add --scope user --transport stdio keynobi -- keynobi --mcp"
      },
      "codex": {
        "clientFound": true,
        "configuredCommand": "/Applications/Keynobi.app/Contents/MacOS/keynobi --mcp",
        "configuredScope": "user",
        "isConfigured": true,
        "setupCommand": "claude mcp add --scope user --transport stdio keynobi -- keynobi --mcp"
      },
      "exePath": "/Applications/Keynobi.app/Contents/MacOS/keynobi",
      "locationProblem": null,
      "setupCommand": "/Applications/Keynobi.app/Contents/MacOS/keynobi --mcp"
    },
    {
      "claude": {
        "clientFound": false,
        "configuredCommand": null,
        "configuredScope": null,
        "isConfigured": false,
        "setupCommand": null
      },
      "codex": {
        "clientFound": false,
        "configuredCommand": null,
        "configuredScope": null,
        "isConfigured": false,
        "setupCommand": null
      },
      "exePath": "/Volumes/Keynobi/Keynobi.app/Contents/MacOS/keynobi",
      "locationProblem": "Keynobi is running from a disk image.",
      "setupCommand": null
    }
  ] satisfies Wire<McpSetupStatus>[],
  MonitorStats: [
    {
      "appMemoryBytes": 123456789,
      "logFolderBytes": 4096,
      "rotationTriggered": false
    }
  ] satisfies Wire<MonitorStats>[],
};

/** Payload type and serialized payload samples of each backend event. */
export const eventFixtures = {
  "build:started": {
    type: "BuildStartedEvent",
    samples: [
      {
        "origin": {
          "kind": "app"
        },
        "projectRoot": "/p",
        "runId": 9001,
        "startedAt": "2026-04-23T10:00:00Z",
        "task": "assembleDebug"
      },
      {
        "origin": {
          "kind": "appQuit"
        },
        "projectRoot": null,
        "runId": 9002,
        "startedAt": "2026-04-23T10:00:00Z",
        "task": "assembleDebug"
      },
      {
        "origin": {
          "clientName": "Claude Code",
          "kind": "agent",
          "sessionId": 2,
          "standalone": false
        },
        "projectRoot": "/p",
        "runId": 9003,
        "startedAt": "2026-04-23T10:00:00Z",
        "task": "assembleDebug"
      },
      {
        "origin": {
          "clientName": null,
          "kind": "agent",
          "sessionId": null,
          "standalone": true
        },
        "projectRoot": null,
        "runId": 9004,
        "startedAt": "2026-04-23T10:00:00Z",
        "task": "assembleDebug"
      }
    ] satisfies Wire<BuildStartedEvent>[],
  },
  "build:lines": {
    type: "BuildLinesEvent",
    samples: [
      {
        "lines": [
          {
            "col": 5,
            "content": "e: file:///p/app/src/main/java/Main.kt:10:5 Unresolved reference: foo",
            "file": "/p/app/src/main/java/Main.kt",
            "kind": "error",
            "line": 10
          },
          {
            "col": null,
            "content": "> Task :app:assembleDebug",
            "file": null,
            "kind": "output",
            "line": null
          }
        ],
        "runId": 9001
      }
    ] satisfies Wire<BuildLinesEvent>[],
  },
  "build:complete": {
    type: "BuildCompleteEvent",
    samples: [
      {
        "cancelled": true,
        "cancelledBy": {
          "kind": "app"
        },
        "durationMs": 4200,
        "errorCount": 0,
        "origin": {
          "kind": "app"
        },
        "recordId": 7,
        "runId": 9001,
        "success": false,
        "task": "assembleDebug",
        "warningCount": 1
      },
      {
        "cancelled": true,
        "cancelledBy": {
          "kind": "appQuit"
        },
        "durationMs": 4200,
        "errorCount": 0,
        "origin": {
          "kind": "appQuit"
        },
        "recordId": 8,
        "runId": 9002,
        "success": false,
        "task": "assembleDebug",
        "warningCount": 1
      },
      {
        "cancelled": true,
        "cancelledBy": {
          "clientName": "Claude Code",
          "kind": "agent",
          "sessionId": 2,
          "standalone": false
        },
        "durationMs": 4200,
        "errorCount": 0,
        "origin": {
          "clientName": "Claude Code",
          "kind": "agent",
          "sessionId": 2,
          "standalone": false
        },
        "recordId": 9,
        "runId": 9003,
        "success": false,
        "task": "assembleDebug",
        "warningCount": 1
      },
      {
        "cancelled": true,
        "cancelledBy": {
          "clientName": null,
          "kind": "agent",
          "sessionId": null,
          "standalone": true
        },
        "durationMs": 4200,
        "errorCount": 0,
        "origin": {
          "clientName": null,
          "kind": "agent",
          "sessionId": null,
          "standalone": true
        },
        "recordId": 10,
        "runId": 9004,
        "success": false,
        "task": "assembleDebug",
        "warningCount": 1
      },
      {
        "cancelled": false,
        "cancelledBy": null,
        "durationMs": 4200,
        "errorCount": 0,
        "origin": null,
        "recordId": 11,
        "runId": 9005,
        "success": true,
        "task": "assembleDebug",
        "warningCount": 1
      }
    ] satisfies Wire<BuildCompleteEvent>[],
  },
  "device:list_changed": {
    type: "DeviceListChangedEvent",
    samples: [
      {
        "devices": [
          {
            "androidVersion": "14",
            "apiLevel": 34,
            "avdName": "Pixel_7_API_34",
            "connectionState": "online",
            "deviceKind": "emulator",
            "model": "sdk_gphone64_arm64",
            "name": "Pixel 7",
            "serial": "emulator-5554"
          },
          {
            "androidVersion": "15",
            "apiLevel": 35,
            "connectionState": "online",
            "deviceKind": "physical",
            "model": "Pixel 7",
            "name": "Pixel 7",
            "serial": "28151FDH2000Q4"
          },
          {
            "androidVersion": null,
            "apiLevel": null,
            "connectionState": "unauthorized",
            "deviceKind": "physical",
            "model": null,
            "name": "ZX1G22ABCD",
            "serial": "ZX1G22ABCD"
          }
        ]
      }
    ] satisfies Wire<DeviceListChangedEvent>[],
  },
  "logcat:entries": {
    type: "ProcessedEntry[]",
    samples: [
      [
        {
          "category": "general",
          "crashGroupId": 41,
          "flags": 5,
          "id": 41,
          "isCrash": true,
          "jsonBody": "{\"ok\":false}",
          "kind": "normal",
          "level": "error",
          "message": "FATAL EXCEPTION: main",
          "package": "com.example.app",
          "pid": 1234,
          "tag": "AndroidRuntime",
          "tid": 1236,
          "timestamp": "2026-04-23T10:00:02.000Z"
        },
        {
          "category": "lifecycle",
          "crashGroupId": null,
          "flags": 0,
          "id": 42,
          "isCrash": false,
          "jsonBody": null,
          "kind": "processDied",
          "level": "info",
          "message": "Process com.example.app has died",
          "package": null,
          "pid": 1300,
          "tag": "ActivityManager",
          "tid": 1300,
          "timestamp": "2026-04-23T10:00:03.000Z"
        }
      ]
    ] satisfies Wire<ProcessedEntry[]>[],
  },
  "logcat:cleared": {
    type: "null",
    samples: [
      null
    ] satisfies Wire<null>[],
  },
  "logcat:reconnecting": {
    type: "null",
    samples: [
      null
    ] satisfies Wire<null>[],
  },
  "logcat:stopped": {
    type: "string",
    samples: [
      "Logcat stopped: adb is not responding"
    ] satisfies Wire<string>[],
  },
  "mcp:sessions_changed": {
    type: "McpAttachedSession[]",
    samples: [
      [
        {
          "clientName": "Claude Code",
          "connectedAt": "2026-04-23T10:00:00Z",
          "id": 2,
          "pid": 4321,
          "project": "/p",
          "version": "0.1.29"
        },
        {
          "clientName": null,
          "connectedAt": "2026-04-23T10:00:00Z",
          "id": 3,
          "pid": null,
          "project": null,
          "version": "0.1.28"
        }
      ]
    ] satisfies Wire<McpAttachedSession[]>[],
  },
  "settings:corrupted": {
    type: "null",
    samples: [
      null
    ] satisfies Wire<null>[],
  },
  "monitor://stats": {
    type: "MonitorStats",
    samples: [
      {
        "appMemoryBytes": 123456789,
        "logFolderBytes": 4096,
        "rotationTriggered": false
      }
    ] satisfies Wire<MonitorStats>[],
  },
};
