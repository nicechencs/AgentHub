import { describe, expect, it } from 'vitest';
import { BRIDGES_PATH } from '@/lib/bridges-path';
import { trayNavigatePath } from './tray-events';

describe('trayNavigatePath', () => {
  it('accepts /routes', () => {
    expect(trayNavigatePath({ path: '/routes' })).toBe('/routes');
  });

  it('accepts BRIDGES_PATH', () => {
    expect(trayNavigatePath({ path: BRIDGES_PATH })).toBe('/routes');
  });

  it('rejects missing, empty, relative, and non-string paths', () => {
    expect(trayNavigatePath(undefined)).toBeNull();
    expect(trayNavigatePath({})).toBeNull();
    expect(trayNavigatePath({ path: '' })).toBeNull();
    expect(trayNavigatePath({ path: 'routes' })).toBeNull();
    expect(trayNavigatePath({ path: './routes' })).toBeNull();
    expect(trayNavigatePath({ path: 1 })).toBeNull();
    expect(trayNavigatePath({ path: null })).toBeNull();
  });
});
