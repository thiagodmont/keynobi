import { createRoot } from "solid-js";
import { afterEach, describe, expect, it, vi } from "vitest";
import { buildEffectiveQueryWithDisabledPills, queryBarPillId } from "@/lib/logcat-query";
import { resolveQueryVariables } from "@/lib/logcat-query-variables";
import { createLogcatQueryController } from "./logcat-query-controller";

describe("createLogcatQueryController", () => {
  afterEach(() => {
    vi.useRealTimers();
  });

  it("debounces query updates and runs the interaction reset hook", async () => {
    vi.useFakeTimers();
    const reset = vi.fn();
    const { controller, dispose } = createRoot((dispose) => {
      const controller = createLogcatQueryController({ onQueryInteractionReset: reset });

      controller.updateQuery("tag:MainActivity");

      expect(reset).toHaveBeenCalledTimes(1);
      expect(controller.query()).toBe("tag:MainActivity");
      expect(controller.debouncedQuery()).toBe("");

      return { controller, dispose };
    });

    await vi.advanceTimersByTimeAsync(150);
    expect(controller.debouncedQuery()).toBe("tag:MainActivity");
    dispose();
  });

  it("tracks and debounces query variable values", async () => {
    vi.useFakeTimers();
    const { controller, dispose } = createRoot((dispose) => {
      const controller = createLogcatQueryController({ onQueryInteractionReset: vi.fn() });

      controller.updateQuery("message:action_${action_name}_done ");
      controller.updateQueryVariableValue("action_name", "checkout");

      expect(controller.queryVariableValues()).toEqual({ action_name: "checkout" });
      expect(controller.debouncedQueryVariableValues()).toEqual({ action_name: "" });

      return { controller, dispose };
    });

    await vi.advanceTimersByTimeAsync(150);
    expect(controller.debouncedQueryVariableValues()).toEqual({ action_name: "checkout" });
    dispose();
  });

  it("reconciles disabled pills when the query changes", () => {
    createRoot((dispose) => {
      const controller = createLogcatQueryController({ onQueryInteractionReset: vi.fn() });

      controller.updateQuery("tag:Alpha tag:Beta ");
      controller.togglePillDisabled(queryBarPillId("tag:Beta", 0));
      expect(
        buildEffectiveQueryWithDisabledPills(controller.query(), controller.disabledPillIds())
      ).toBe("tag:Alpha ");

      controller.updateQuery("tag:Alpha ");
      expect(controller.disabledPillIds().size).toBe(0);

      dispose();
    });
  });

  it("restores a persisted query without waiting for the debounce timer", () => {
    createRoot((dispose) => {
      const controller = createLogcatQueryController({ onQueryInteractionReset: vi.fn() });

      controller.restoreQuery("message:action_${action_name}_done ");

      expect(controller.query()).toBe("message:action_${action_name}_done ");
      expect(controller.debouncedQuery()).toBe("message:action_${action_name}_done ");
      expect(controller.queryVariableValues()).toEqual({ action_name: "" });
      expect(
        resolveQueryVariables(controller.debouncedQuery(), controller.queryVariableValues())
      ).toBe("message:action_${action_name}_done ");

      dispose();
    });
  });
});
