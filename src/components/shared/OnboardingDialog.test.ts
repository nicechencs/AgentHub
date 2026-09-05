import { readFileSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { describe, expect, it } from 'vitest';

const dir = path.dirname(fileURLToPath(import.meta.url));

function source(name: string): string {
  return readFileSync(path.join(dir, name), 'utf8');
}

describe('OnboardingDialog first-run usage', () => {
  it('starts on the usage step and can skip the whole guide', () => {
    const dialog = source('OnboardingDialog.tsx');
    expect(dialog).toContain("useState<Step>('usage')");
    expect(dialog).toContain('OnboardingUsageStep');
    expect(dialog).toContain('skipGuide');
    expect(dialog).toContain('continueFromUsage');
    expect(dialog).toContain('hasOnboardingUsageChoice');
    expect(dialog).toContain('notifyOnboardingFinished');
    expect(dialog.indexOf("step === 'usage'")).toBeLessThan(dialog.indexOf("step === 'env'"));
  });

  it('applies sidebar visibility from the choice and still uses Settings toggles', () => {
    const dialog = source('OnboardingDialog.tsx');
    expect(dialog).toContain('navVisibilityForUsage');
    expect(dialog).toContain('setRoutesNavVisible');
    expect(dialog).toContain('setSub2apiNavVisible');
    expect(dialog).toContain('applyUsage');
    const prefs = readFileSync(path.join(dir, '../../pages/settings/PreferencesPanel.tsx'), 'utf8');
    expect(prefs).toContain("t('settings.general.routesNavVisibleLabel')");
    expect(prefs).toContain("t('settings.general.sub2apiNavVisibleLabel')");
    expect(prefs).toContain('setRoutesNavVisible');
    expect(prefs).toContain('setSub2apiNavVisible');
  });
});
