/**
 * Pure official-endpoint checkbox transitions (node-testable, no DOM).
 * Checking snapshots custom values; unchecking restores them.
 */
import type { ProviderFormVars } from '@/lib/provider-detect';

export type OfficialToggleForm = {
  vars: ProviderFormVars;
  configText: string;
  configFormat: 'json' | 'toml';
};

export type OfficialToggleSnapshot = OfficialToggleForm;

function cloneForm(form: OfficialToggleForm): OfficialToggleForm {
  return {
    vars: { ...form.vars },
    configText: form.configText,
    configFormat: form.configFormat,
  };
}

/**
 * Next form after toggling official-endpoint.
 *
 * - checked=true: snapshot current custom values (once), then show official defaults.
 * - checked=false: restore snapshot when present; otherwise keep current.
 *   Never invent a your-relay / TOML placeholder here.
 */
export function officialToggleNext(args: {
  checked: boolean;
  current: OfficialToggleForm;
  snapshot: OfficialToggleSnapshot | null;
  official: OfficialToggleForm | null;
}): OfficialToggleForm & { snapshot: OfficialToggleSnapshot | null } {
  if (args.checked) {
    const snapshot = args.snapshot ? cloneForm(args.snapshot) : cloneForm(args.current);
    if (args.official) {
      const next = cloneForm(args.official);
      return { snapshot, ...next };
    }
    const current = cloneForm(args.current);
    return { snapshot, ...current };
  }

  if (args.snapshot) {
    const restored = cloneForm(args.snapshot);
    return { snapshot: cloneForm(args.snapshot), ...restored };
  }

  const current = cloneForm(args.current);
  return { snapshot: null, ...current };
}
