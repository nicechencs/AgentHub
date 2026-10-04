import { describe, expect, it } from 'vitest';
import { isPiAbsoluteLocalSource } from './plugin-install-source';

describe('isPiAbsoluteLocalSource', () => {
  it('accepts Unix, Windows, UNC, and home-expanded absolute sources', () => {
    expect(isPiAbsoluteLocalSource('/opt/pi-extension')).toBe(true);
    expect(isPiAbsoluteLocalSource('C:\\pi\\extension')).toBe(true);
    expect(isPiAbsoluteLocalSource('\\\\server\\share\\extension')).toBe(true);
    expect(isPiAbsoluteLocalSource('~/src/pi-extension')).toBe(true);
  });

  it('rejects relative local sources that cannot survive the isolated working directory', () => {
    expect(isPiAbsoluteLocalSource('extension')).toBe(false);
    expect(isPiAbsoluteLocalSource('./extension')).toBe(false);
    expect(isPiAbsoluteLocalSource('../extension')).toBe(false);
  });
});
