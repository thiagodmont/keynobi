import { createSignal } from "solid-js";
import { settingsState } from "@/stores/settings.store";

const [onboardingWizardOpen, setOnboardingWizardOpen] = createSignal(false);

export { onboardingWizardOpen };

export function openOnboardingWizard(): void {
  setOnboardingWizardOpen(true);
}

export function closeOnboardingWizard(): void {
  setOnboardingWizardOpen(false);
}

/** Call once after settings have been loaded from disk. Opens the wizard if the user has not completed onboarding. */
export function tryOpenOnboardingAfterLoad(): void {
  if (!settingsState.onboardingCompleted) {
    setOnboardingWizardOpen(true);
  }
}
