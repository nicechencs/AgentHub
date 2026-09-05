import { loadBool, saveBool, StorageKey } from '@/lib/ui-preferences';

type Listener = () => void;
const listeners = new Set<Listener>();

function emit(): void {
  for (const listener of listeners) listener();
}

/** True after first-run onboarding, until the chrome hint is dismissed. */
export function isChromeHintPending(): boolean {
  return (
    loadBool(StorageKey.onboardingDone, false) && !loadBool(StorageKey.chromeHintDismissed, false)
  );
}

export function getChromeHintSnapshot(): boolean {
  return isChromeHintPending();
}

export function subscribeChromeHint(listener: Listener): () => void {
  listeners.add(listener);
  return () => {
    listeners.delete(listener);
  };
}

export function dismissChromeHint(): void {
  if (loadBool(StorageKey.chromeHintDismissed, false)) return;
  saveBool(StorageKey.chromeHintDismissed, true);
  emit();
}

/** Call after first-run onboarding writes `onboardingDone`. */
export function notifyOnboardingFinished(): void {
  emit();
}

export const CHROME_HINT_SHOW_DELAY_MS = 400;
export const CHROME_HINT_AUTO_DISMISS_MS = 10_000;
