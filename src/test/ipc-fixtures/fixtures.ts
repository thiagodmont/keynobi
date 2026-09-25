// Generated from the Rust IPC types by `npm run generate:ipc-fixtures`. Do not edit.
import type {
  AppError,
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
  Device,
  DeviceDefinition,
  DeviceListChangedEvent,
  LogStats,
  McpActivityEntry,
  McpAttachedSession,
  McpServerStatus,
  McpSetupStatus,
  MonitorStats,
  ProcessedEntry,
  ProjectAppInfo,
  ProjectEntry,
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
      "versionName": "1.2.3"
    },
    {
      "applicationId": null,
      "versionCode": null,
      "versionName": null
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
      "emulatorFound": true,
      "gradleWrapperFound": true,
      "javaBinUsed": "/jdk/bin/java",
      "javaExecutableFound": true,
      "javaHome": "/jdk",
      "javaMajorVersion": 17,
      "javaSource": "androidStudio",
      "javaVersion": "openjdk version \"17.0.9\"",
      "lspSystemDirOk": true,
      "studioCommandFound": false
    },
    {
      "adbFound": false,
      "adbVersion": null,
      "androidSdkValid": false,
      "emulatorFound": false,
      "gradleWrapperFound": false,
      "javaBinUsed": "java",
      "javaExecutableFound": false,
      "javaHome": null,
      "javaMajorVersion": null,
      "javaSource": null,
      "javaVersion": null,
      "lspSystemDirOk": true,
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
      "cancelledBy": {
        "kind": "app"
      },
      "errors": [],
      "id": 7,
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
      "cancelledBy": {
        "kind": "appQuit"
      },
      "errors": [],
      "id": 8,
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
      "cancelledBy": {
        "clientName": "Claude Code",
        "kind": "agent",
        "sessionId": 2,
        "standalone": false
      },
      "errors": [],
      "id": 9,
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
      "cancelledBy": {
        "clientName": null,
        "kind": "agent",
        "sessionId": null,
        "standalone": true
      },
      "errors": [],
      "id": 10,
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
    }
  ] satisfies Wire<BuildRecord>[],
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
      "connectionState": "online",
      "deviceKind": "emulator",
      "model": "sdk_gphone64_arm64",
      "name": "Pixel 7",
      "serial": "emulator-5554"
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
  DeviceListChangedEvent: [
    {
      "devices": [
        {
          "androidVersion": "14",
          "apiLevel": 34,
          "connectionState": "online",
          "deviceKind": "emulator",
          "model": "sdk_gphone64_arm64",
          "name": "Pixel 7",
          "serial": "emulator-5554"
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
      "project": "/p"
    },
    {
      "clientName": null,
      "connectedAt": "2026-04-23T10:00:00Z",
      "id": 3,
      "pid": null,
      "project": null
    }
  ] satisfies Wire<McpAttachedSession>[],
  McpServerStatus: [
    {
      "attached": [
        {
          "clientName": "Claude Code",
          "connectedAt": "2026-04-23T10:00:00Z",
          "id": 2,
          "pid": 4321,
          "project": "/p"
        },
        {
          "clientName": null,
          "connectedAt": "2026-04-23T10:00:00Z",
          "id": 3,
          "pid": null,
          "project": null
        }
      ],
      "listening": true,
      "standalone": [
        {
          "pid": 5555,
          "project": "/p",
          "reason": "the Keynobi app is not running",
          "startedAt": "2026-04-23T10:00:00Z"
        },
        {
          "pid": 5556,
          "project": null,
          "reason": "the Keynobi app is not running",
          "startedAt": "2026-04-23T10:00:00Z"
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
        "isConfigured": true,
        "setupCommand": "claude mcp add --transport stdio keynobi -- keynobi --mcp"
      },
      "codex": {
        "clientFound": true,
        "configuredCommand": "/Applications/Keynobi.app/Contents/MacOS/keynobi --mcp",
        "isConfigured": true,
        "setupCommand": "claude mcp add --transport stdio keynobi -- keynobi --mcp"
      },
      "exePath": "/Applications/Keynobi.app/Contents/MacOS/keynobi",
      "setupCommand": "/Applications/Keynobi.app/Contents/MacOS/keynobi --mcp"
    },
    {
      "claude": {
        "clientFound": false,
        "configuredCommand": null,
        "isConfigured": false,
        "setupCommand": "claude mcp add --transport stdio keynobi -- keynobi --mcp"
      },
      "codex": {
        "clientFound": false,
        "configuredCommand": null,
        "isConfigured": false,
        "setupCommand": "claude mcp add --transport stdio keynobi -- keynobi --mcp"
      },
      "exePath": "/Applications/Keynobi.app/Contents/MacOS/keynobi",
      "setupCommand": "/Applications/Keynobi.app/Contents/MacOS/keynobi --mcp"
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
            "connectionState": "online",
            "deviceKind": "emulator",
            "model": "sdk_gphone64_arm64",
            "name": "Pixel 7",
            "serial": "emulator-5554"
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
          "project": "/p"
        },
        {
          "clientName": null,
          "connectedAt": "2026-04-23T10:00:00Z",
          "id": 3,
          "pid": null,
          "project": null
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
