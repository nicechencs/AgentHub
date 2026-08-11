import { describe, expect, it } from 'vitest';
import { appIconUrl } from './AppLogo';

describe('appIconUrl', () => {
  it('resolves under Vite BASE_URL', () => {
    expect(appIconUrl()).toMatch(/app-icon\.png$/);
    expect(appIconUrl()).toContain('app-icon.png');
  });
});
