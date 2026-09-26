import { describe, it, expect, afterEach, vi } from "vitest";
import { render, screen, cleanup } from "@solidjs/testing-library";
import { VariantSelectorPill } from "./VariantSelector";
import {
  resetRunConfigurationsForTests,
  setRunConfigurations,
} from "@/stores/run-configurations.store";
import { makeProjectRunConfigurations, makeRunConfiguration } from "@/test/factories/build";

vi.mock("@/stores/variant.store", () => ({
  variantState: {
    module: ":wear",
    activeVariant: "release",
    variants: [{ name: "release" }],
    loading: false,
    gradleLoading: false,
  },
  loadVariants: vi.fn(() => Promise.resolve()),
  selectVariant: vi.fn(() => Promise.resolve()),
}));

describe("VariantSelectorPill", () => {
  afterEach(() => {
    cleanup();
    resetRunConfigurationsForTests();
  });

  it("shows the variant alone when no run configuration is loaded", () => {
    render(() => <VariantSelectorPill />);
    expect(screen.getByRole("button").textContent).toContain("release");
    expect(screen.getByRole("button").textContent).not.toContain("·");
  });

  it("shows the active run configuration's module and variant", () => {
    setRunConfigurations(
      "/projects/app",
      makeProjectRunConfigurations([
        makeRunConfiguration({ name: "Wear", module: ":wear", variant: "release" }),
      ])
    );
    render(() => <VariantSelectorPill />);
    expect(screen.getByRole("button").textContent).toContain(":wear · release");
  });
});
