import { fireEvent, render, screen } from "@solidjs/testing-library";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { LogcatFilterControls } from "./LogcatFilterControls";
import { buildQueryBarPillRefs } from "@/lib/logcat-query";

function renderFilterControls(overrides: Partial<Parameters<typeof LogcatFilterControls>[0]> = {}) {
  const props: Parameters<typeof LogcatFilterControls>[0] = {
    query: "",
    knownTags: [],
    knownPackages: ["com.example.app"],
    hasAgeFilter: false,
    activeAge: null,
    activePackage: null,
    isFiltered: false,
    showLifecycle: true,
    onQueryChange: vi.fn(),
    onAgeSelect: vi.fn(),
    onPackageSelect: vi.fn(),
    onToggleLifecycle: vi.fn(),
    onClear: vi.fn(),
    ...overrides,
  };

  return {
    ...render(() => <LogcatFilterControls {...props} />),
    props,
  };
}

describe("LogcatFilterControls", () => {
  beforeEach(() => {
    localStorage.clear();
  });

  it("renders the query row before the quick-filter row", () => {
    renderFilterControls();

    const queryRow = screen.getByTestId("logcat-filter-query-row");
    const quickRow = screen.getByTestId("logcat-filter-quick-row");
    const siblings = Array.from(queryRow.parentElement?.children ?? []);

    expect(siblings.indexOf(queryRow)).toBeLessThan(siblings.indexOf(quickRow));
  });

  it("renders filter presets in the quick-filter row", () => {
    renderFilterControls();

    const quickRow = screen.getByTestId("logcat-filter-quick-row");
    expect(quickRow.contains(screen.getByTitle("Filter presets"))).toBe(true);
  });

  it("selects age quick filters", () => {
    const onAgeSelect = vi.fn();
    renderFilterControls({ onAgeSelect });

    fireEvent.click(screen.getByText("5m"));
    expect(onAgeSelect).toHaveBeenCalledWith("5m");

    fireEvent.click(screen.getByText("All"));
    expect(onAgeSelect).toHaveBeenCalledWith(null);
  });

  it("shows lifecycle quick filter as active by default", () => {
    renderFilterControls();

    const button = screen.getByRole("button", { name: "Lifecycle" });

    expect(button.getAttribute("aria-pressed")).toBe("true");
  });

  it("toggles lifecycle quick filter", () => {
    const onToggleLifecycle = vi.fn();
    renderFilterControls({ showLifecycle: false, onToggleLifecycle });

    const button = screen.getByRole("button", { name: "Lifecycle" });
    expect(button.getAttribute("aria-pressed")).toBe("false");

    fireEvent.click(button);

    expect(onToggleLifecycle).toHaveBeenCalledOnce();
  });

  it("selects package filters from the package dropdown", () => {
    const onPackageSelect = vi.fn();
    renderFilterControls({ onPackageSelect });

    fireEvent.click(screen.getByTitle("Filter by package"));
    fireEvent.click(screen.getByText("com.example.app"));

    expect(onPackageSelect).toHaveBeenCalledWith("com.example.app");
  });

  it("clears active filters", () => {
    const onClear = vi.fn();
    renderFilterControls({ isFiltered: true, query: "level:error ", onClear });

    fireEvent.click(screen.getByTitle("Clear all filters"));

    expect(onClear).toHaveBeenCalledOnce();
  });

  it("shows variable inputs between the query row and quick filters", () => {
    renderFilterControls({
      isFiltered: true,
      query: "message:${action_name} tag:${screen} ",
      variableValues: { action_name: "tap" },
    });

    const queryRow = screen.getByTestId("logcat-filter-query-row");
    const variableRow = screen.getByTestId("logcat-filter-variable-row");
    const quickRow = screen.getByTestId("logcat-filter-quick-row");
    const siblings = Array.from(queryRow.parentElement?.children ?? []);

    expect(siblings.indexOf(queryRow)).toBeLessThan(siblings.indexOf(variableRow));
    expect(siblings.indexOf(variableRow)).toBeLessThan(siblings.indexOf(quickRow));
    expect((screen.getByTitle("Variable action_name") as HTMLInputElement).value).toBe("tap");
    expect((screen.getByTitle("Variable screen") as HTMLInputElement).value).toBe("");
  });

  it("updates variable values from the variable row", () => {
    const onVariableValueChange = vi.fn();
    renderFilterControls({
      isFiltered: true,
      query: "message:some_prefix_${action_name} ",
      variableValues: { action_name: "" },
      onVariableValueChange,
    });

    fireEvent.input(screen.getByTitle("Variable action_name"), {
      target: { value: "checkout" },
    });

    expect(onVariableValueChange).toHaveBeenCalledWith("action_name", "checkout");
  });

  it("shows predefined variables even before the query references them", () => {
    renderFilterControls({
      isFiltered: false,
      query: "",
      variableValues: { action_name: "checkout" },
    });

    expect(screen.getByTestId("logcat-filter-variable-row")).not.toBeNull();
    expect((screen.getByTitle("Variable action_name") as HTMLInputElement).value).toBe("checkout");
  });

  it("creates variables from the manager before they are used in the query", () => {
    const onVariableValueChange = vi.fn();
    renderFilterControls({
      variableValues: {},
      onVariableValueChange,
    });

    fireEvent.click(screen.getByTitle("Manage filter variables"));
    fireEvent.input(screen.getByPlaceholderText("variable_name"), {
      target: { value: "action_name" },
    });
    fireEvent.input(screen.getByPlaceholderText("Initial value"), {
      target: { value: "checkout" },
    });
    fireEvent.click(screen.getByRole("button", { name: "Add variable" }));

    expect(onVariableValueChange).toHaveBeenCalledWith("action_name", "checkout");
  });

  it("inserts an existing variable token from the manager", () => {
    const onVariableInsert = vi.fn();
    renderFilterControls({
      variableValues: { action_name: "checkout" },
      onVariableInsert,
    });

    fireEvent.click(screen.getByTitle("Manage filter variables"));
    fireEvent.click(screen.getByTitle("Insert variable action_name"));

    expect(onVariableInsert).toHaveBeenCalledWith("action_name");
  });

  it("deletes variables from the manager", () => {
    const onVariableDelete = vi.fn();
    renderFilterControls({
      variableValues: { action_name: "checkout" },
      onVariableDelete,
    });

    fireEvent.click(screen.getByTitle("Manage filter variables"));
    fireEvent.click(screen.getByTitle("Delete variable action_name"));

    expect(onVariableDelete).toHaveBeenCalledWith("action_name");
  });

  it("hides the direct save button until a filter is active", () => {
    renderFilterControls({ isFiltered: false, query: "" });

    expect(screen.queryByTitle("Save current filter")).toBeNull();
  });

  it("saves the active query from the query row and shows it in the filters menu", () => {
    renderFilterControls({
      isFiltered: true,
      query: "level:error ",
    });

    fireEvent.click(screen.getByTitle("Save current filter"));
    fireEvent.input(screen.getByPlaceholderText("Filter name…"), {
      target: { value: "Errors" },
    });
    fireEvent.click(screen.getAllByRole("button", { name: "Save" })[1]!);
    fireEvent.click(screen.getByTitle("Filter presets"));

    expect(screen.getByText("Errors")).not.toBeNull();
  });

  it("opens the direct save name box with an opaque panel background", () => {
    renderFilterControls({
      isFiltered: true,
      query: "level:error ",
    });

    fireEvent.click(screen.getByTitle("Save current filter"));

    const box = screen.getByPlaceholderText("Filter name…").parentElement;
    expect(box?.getAttribute("style")).toContain("background:var(--bg-secondary)");
  });

  it("saves the effective active query when a pill is temporarily disabled", () => {
    const onQueryChange = vi.fn();
    const refs = buildQueryBarPillRefs(["tag:Alpha", "tag:Beta"]);
    const beta = refs.find((ref) => ref.token === "tag:Beta")!;

    renderFilterControls({
      isFiltered: true,
      query: "tag:Alpha tag:Beta ",
      disabledPillIds: new Set([beta.id]),
      onQueryChange,
    });

    fireEvent.click(screen.getByTitle("Save current filter"));
    fireEvent.input(screen.getByPlaceholderText("Filter name…"), {
      target: { value: "Alpha only" },
    });
    fireEvent.click(screen.getAllByRole("button", { name: "Save" })[1]!);
    fireEvent.click(screen.getByTitle("Filter presets"));
    fireEvent.click(screen.getByText("Alpha only"));

    expect(onQueryChange).toHaveBeenCalledWith("tag:Alpha ");
  });

  it("saves variable filters as reusable templates instead of resolved values", () => {
    const onQueryChange = vi.fn();

    renderFilterControls({
      isFiltered: true,
      query: "message:action_${action_name}_done ",
      variableValues: { action_name: "checkout" },
      onQueryChange,
    });

    fireEvent.click(screen.getByTitle("Save current filter"));
    fireEvent.input(screen.getByPlaceholderText("Filter name…"), {
      target: { value: "Action template" },
    });
    fireEvent.click(screen.getAllByRole("button", { name: "Save" })[1]!);
    fireEvent.click(screen.getByTitle("Filter presets"));
    fireEvent.click(screen.getByText("Action template"));

    expect(onQueryChange).toHaveBeenCalledWith("message:action_${action_name}_done ");
  });
});
