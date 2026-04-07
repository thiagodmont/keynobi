import { describe, it, expect, beforeEach } from "vitest";
import {
  onboardingWizardOpen,
  openOnboardingWizard,
  closeOnboardingWizard,
  tryOpenOnboardingAfterLoad,
} from "@/stores/onboarding.store";
import { setAppSetting } from "@/stores/settings.store";

describe("onboarding.store", () => {
  beforeEach(() => {
    closeOnboardingWizard();
    setAppSetting("onboardingCompleted", false);
  });

  it("tryOpenOnboardingAfterLoad opens when onboarding is not completed", () => {
    setAppSetting("onboardingCompleted", false);
    tryOpenOnboardingAfterLoad();
    expect(onboardingWizardOpen()).toBe(true);
  });

  it("tryOpenOnboardingAfterLoad does not open when onboarding is already completed", () => {
    setAppSetting("onboardingCompleted", true);
    tryOpenOnboardingAfterLoad();
    expect(onboardingWizardOpen()).toBe(false);
  });

  it("openOnboardingWizard toggles visibility", () => {
    expect(onboardingWizardOpen()).toBe(false);
    openOnboardingWizard();
    expect(onboardingWizardOpen()).toBe(true);
    closeOnboardingWizard();
    expect(onboardingWizardOpen()).toBe(false);
  });

  it("openOnboardingWizard opens even when onboarding is already marked completed (re-run from command palette)", () => {
    setAppSetting("onboardingCompleted", true);
    expect(onboardingWizardOpen()).toBe(false);
    openOnboardingWizard();
    expect(onboardingWizardOpen()).toBe(true);
    closeOnboardingWizard();
  });

  it("tryOpenOnboardingAfterLoad does not close a manually opened wizard when onboarding is completed", () => {
    setAppSetting("onboardingCompleted", true);
    openOnboardingWizard();
    tryOpenOnboardingAfterLoad();
    expect(onboardingWizardOpen()).toBe(true);
    closeOnboardingWizard();
  });
});
