import { describe, it, expect, vi } from "vitest";
import { fireEvent, render } from "@solidjs/testing-library";
import { QueryBar } from "./QueryBar";
import { buildQueryBarPillRefs } from "@/lib/logcat-query";

// ── QueryBar keyboard behavior ───────────────────────────────────────────────

describe("QueryBar — Enter keyboard commit", () => {
  it("commits the typed draft as a pill when no autocomplete suggestion is selected", () => {
    const changes: string[] = [];
    const { container } = render(() =>
      QueryBar({
        value: "tag:OkHttp",
        onChange: (q) => changes.push(q),
        knownTags: [],
        knownPackages: [],
      })
    );
    const input = container.querySelector("input") as HTMLInputElement;

    fireEvent.focus(input);
    fireEvent.keyDown(input, { key: "Enter" });

    expect(changes[changes.length - 1]).toBe("tag:OkHttp ");
  });

  it("balances a multi-word message draft and commits it as one pill", () => {
    const changes: string[] = [];
    const { container } = render(() =>
      QueryBar({
        value: 'message:"hello world',
        onChange: (q) => changes.push(q),
        knownTags: [],
        knownPackages: [],
      })
    );
    const input = container.querySelector("input") as HTMLInputElement;

    fireEvent.focus(input);
    fireEvent.keyDown(input, { key: "Enter" });

    expect(changes[changes.length - 1]).toBe('message:"hello world" ');
  });

  it("keeps Enter accepting the selected autocomplete suggestion", () => {
    const changes: string[] = [];
    const { container } = render(() =>
      QueryBar({
        value: "tag:Ok",
        onChange: (q) => changes.push(q),
        knownTags: ["OkHttp"],
        knownPackages: [],
      })
    );
    const input = container.querySelector("input") as HTMLInputElement;

    fireEvent.focus(input);
    fireEvent.keyDown(input, { key: "Enter" });

    expect(changes[changes.length - 1]).toBe("tag:OkHttp ");
  });
});

describe("QueryBar — clickable connectors", () => {
  it("toggles only the clicked AND connector to OR", async () => {
    const changes: string[] = [];
    const { getByRole, queryByText } = render(() =>
      QueryBar({
        value: "level:error tag:App | is:crash ",
        onChange: (q) => changes.push(q),
        knownTags: [],
        knownPackages: [],
      })
    );

    fireEvent.click(getByRole("button", { name: "Change AND to OR" }));
    await Promise.resolve();

    expect(changes[changes.length - 1]).toBe("level:error | tag:App | is:crash ");
    expect(queryByText("level:")).toBeNull();
  });

  it("toggles only the clicked OR connector to AND", async () => {
    const changes: string[] = [];
    const { getAllByRole, queryByText } = render(() =>
      QueryBar({
        value: "level:error | tag:App | is:crash ",
        onChange: (q) => changes.push(q),
        knownTags: [],
        knownPackages: [],
      })
    );

    fireEvent.click(getAllByRole("button", { name: "Change OR to AND" })[0]!);
    await Promise.resolve();

    expect(changes[changes.length - 1]).toBe("level:error tag:App | is:crash ");
    expect(queryByText("level:")).toBeNull();
  });

  it("closes open suggestions when toggling an AND connector", async () => {
    const { container, getByRole, getByText, queryByText } = render(() =>
      QueryBar({
        value: "level:error tag:App ",
        onChange: vi.fn(),
        knownTags: [],
        knownPackages: [],
      })
    );
    const input = container.querySelector("input") as HTMLInputElement;

    fireEvent.focus(input);
    expect(getByText("level:")).not.toBeNull();

    fireEvent.click(getByRole("button", { name: "Change AND to OR" }));
    await Promise.resolve();

    expect(queryByText("level:")).toBeNull();
  });

  it("toggles an AND connector after an empty OR group", async () => {
    const changes: string[] = [];
    const { getByRole } = render(() =>
      QueryBar({
        value: "level:error | | tag:App is:crash ",
        onChange: (q) => changes.push(q),
        knownTags: [],
        knownPackages: [],
      })
    );

    fireEvent.click(getByRole("button", { name: "Change AND to OR" }));
    await Promise.resolve();

    expect(changes[changes.length - 1]).toBe("level:error | tag:App | is:crash ");
  });
});

describe("QueryBar — sparse OR groups", () => {
  it("toggles a visible OR connector after an empty OR group", async () => {
    const changes: string[] = [];
    const { getByRole } = render(() =>
      QueryBar({
        value: "level:error | | tag:App ",
        onChange: (q) => changes.push(q),
        knownTags: [],
        knownPackages: [],
      })
    );

    fireEvent.click(getByRole("button", { name: "Change OR to AND" }));
    await Promise.resolve();

    expect(changes[changes.length - 1]).toBe("level:error tag:App ");
  });

  it("removes a pill after an empty OR group", () => {
    const changes: string[] = [];
    const { getAllByTitle } = render(() =>
      QueryBar({
        value: "level:error | | tag:App ",
        onChange: (q) => changes.push(q),
        knownTags: [],
        knownPackages: [],
      })
    );

    fireEvent.mouseDown(getAllByTitle("Remove filter")[1]!);

    expect(changes[changes.length - 1]).toBe("level:error ");
  });
});

describe("QueryBar — temporarily disabled pills", () => {
  it("toggles a disabled pill without removing or editing it", () => {
    const refs = buildQueryBarPillRefs(["level:error", "tag:App"]);
    const tagRef = refs.find((ref) => ref.token === "tag:App")!;
    const toggled: string[] = [];

    const { getByRole } = render(() =>
      QueryBar({
        value: "level:error tag:App ",
        onChange: vi.fn(),
        knownTags: [],
        knownPackages: [],
        disabledPillIds: new Set([tagRef.id]),
        onTogglePillDisabled: (id) => toggled.push(id),
      })
    );

    fireEvent.click(getByRole("button", { name: "Re-enable filter tag:App" }));

    expect(toggled).toEqual([tagRef.id]);
  });

  it("keeps disabled pills editable", () => {
    const refs = buildQueryBarPillRefs(["level:error", "tag:App"]);
    const tagRef = refs.find((ref) => ref.token === "tag:App")!;

    const { getByText, getByDisplayValue } = render(() =>
      QueryBar({
        value: "level:error tag:App ",
        onChange: vi.fn(),
        knownTags: [],
        knownPackages: [],
        disabledPillIds: new Set([tagRef.id]),
        onTogglePillDisabled: vi.fn(),
      })
    );

    fireEvent.mouseDown(getByText("tag:App"));

    expect(getByDisplayValue("tag:App")).not.toBeNull();
  });
});
