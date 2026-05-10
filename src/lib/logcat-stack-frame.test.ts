import { describe, expect, it } from "vitest";
import { isProjectFrame, isStackTraceLine, parseStackFrame } from "./logcat-stack-frame";

describe("isStackTraceLine", () => {
  it("detects 'at ' prefix", () => {
    expect(isStackTraceLine("  at com.example.Foo.bar(Foo.kt:42)")).toBe(true);
  });

  it("detects Caused by", () => {
    expect(isStackTraceLine("Caused by: java.lang.IllegalStateException")).toBe(true);
  });

  it("detects omitted frames", () => {
    expect(isStackTraceLine("\t... 12 more")).toBe(true);
  });

  it("ignores normal messages", () => {
    expect(isStackTraceLine("Hello world")).toBe(false);
  });
});

describe("parseStackFrame", () => {
  it("parses a Kotlin frame with tab indent", () => {
    const f = parseStackFrame("\tat com.example.app.MainActivity.onCreate(MainActivity.kt:42)");
    expect(f).not.toBeNull();
    expect(f!.filename).toBe("MainActivity.kt");
    expect(f!.line).toBe(42);
    expect(f!.packagePath).toBe("com.example.app");
    expect(f!.classPath).toBe("com.example.app.MainActivity");
  });

  it("parses a Java frame with spaces indent", () => {
    const f = parseStackFrame("  at android.app.Activity.performCreate(Activity.java:8290)");
    expect(f).not.toBeNull();
    expect(f!.filename).toBe("Activity.java");
    expect(f!.line).toBe(8290);
    expect(f!.packagePath).toBe("android.app");
  });

  it("returns null for Caused by lines", () => {
    const f = parseStackFrame("Caused by: java.lang.NullPointerException");
    expect(f).toBeNull();
  });

  it("returns null for ... N more lines", () => {
    const f = parseStackFrame("\t... 5 more");
    expect(f).toBeNull();
  });

  it("returns null for non-frame lines", () => {
    const f = parseStackFrame("E/AndroidRuntime: FATAL EXCEPTION: main");
    expect(f).toBeNull();
  });
});

describe("isProjectFrame", () => {
  it("returns true for a user package", () => {
    expect(isProjectFrame("com.example.app.MainActivity")).toBe(true);
    expect(isProjectFrame("io.mycompany.feature.SomeClass")).toBe(true);
  });

  it("returns false for android. prefix", () => {
    expect(isProjectFrame("android.app.Activity")).toBe(false);
    expect(isProjectFrame("android.view.View")).toBe(false);
  });

  it("returns false for androidx. prefix", () => {
    expect(isProjectFrame("androidx.lifecycle.ViewModel")).toBe(false);
  });

  it("returns false for com.android. prefix", () => {
    expect(isProjectFrame("com.android.internal.os.ZygoteInit")).toBe(false);
  });

  it("returns false for kotlin. and kotlinx. prefixes", () => {
    expect(isProjectFrame("kotlin.jvm.internal.Intrinsics")).toBe(false);
    expect(isProjectFrame("kotlinx.coroutines.CoroutineScope")).toBe(false);
  });

  it("returns false for java. and javax. prefixes", () => {
    expect(isProjectFrame("java.lang.Thread")).toBe(false);
    expect(isProjectFrame("javax.inject.Inject")).toBe(false);
  });

  it("returns false for com.google.android. prefix", () => {
    expect(isProjectFrame("com.google.android.gms.common.GoogleApiAvailability")).toBe(false);
  });
});
