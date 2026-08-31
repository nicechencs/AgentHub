import { readFileSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { afterEach, describe, expect, it, vi } from 'vitest';
import {
  mergeStoredColumnWidths,
  persistColumnWidths,
  readStoredColumnWidths,
} from './table-column-model';

const store = new Map<string, string>();

afterEach(() => {
  store.clear();
  vi.unstubAllGlobals();
});

function stubStorage() {
  vi.stubGlobal('localStorage', {
    getItem: (key: string) => store.get(key) ?? null,
    setItem: (key: string, value: string) => {
      store.set(key, value);
    },
    removeItem: (key: string) => {
      store.delete(key);
    },
  });
}

const defaults = { name: 200, endpoint: 360 };
const minByKey = { name: 120, endpoint: 160 };

describe('mergeStoredColumnWidths', () => {
  it('returns defaults when stored is missing or not a map', () => {
    expect(mergeStoredColumnWidths(null, defaults, minByKey)).toEqual(defaults);
    expect(mergeStoredColumnWidths('nope', defaults, minByKey)).toEqual(defaults);
    expect(mergeStoredColumnWidths(['x'], defaults, minByKey)).toEqual(defaults);
  });

  it('keeps spec keys, clamps to minWidth, and ignores extras', () => {
    expect(
      mergeStoredColumnWidths(
        { name: 80, endpoint: 480.6, leftover: 12 },
        defaults,
        minByKey,
      ),
    ).toEqual({ name: 120, endpoint: 481 });
  });

  it('falls back per-column when a stored value is non-numeric', () => {
    expect(
      mergeStoredColumnWidths({ name: 'wide', endpoint: 400 }, defaults, minByKey),
    ).toEqual({ name: 200, endpoint: 400 });
  });
});

describe('column width persistence', () => {
  it('round-trips a width map through localStorage', () => {
    stubStorage();
    persistColumnWidths('agenthub.test.cols', { name: 240, endpoint: 400 });
    expect(readStoredColumnWidths('agenthub.test.cols', defaults, minByKey)).toEqual({
      name: 240,
      endpoint: 400,
    });
  });

  it('returns defaults when nothing is stored', () => {
    stubStorage();
    expect(readStoredColumnWidths('agenthub.test.cols', defaults, minByKey)).toEqual(defaults);
  });

  it('returns defaults when stored JSON is invalid', () => {
    stubStorage();
    store.set('agenthub.test.cols', '{');
    expect(readStoredColumnWidths('agenthub.test.cols', defaults, minByKey)).toEqual(defaults);
  });
});

describe('resizable table wiring', () => {
  const files = [
    'src/pages/dashboard/UsageDetailsTable.tsx',
    'src/pages/mcp/McpServerTable.tsx',
    'src/pages/skills/SkillMarketTable.tsx',
    'src/pages/skills/SkillMatrix.tsx',
    'src/pages/routes/pool/PoolAuthorizationList.tsx',
  ];

  it('every useColumnWidths call passes a storage key', () => {
    const dir = path.dirname(fileURLToPath(import.meta.url));
    for (const rel of files) {
      const src = readFileSync(path.join(dir, '../../..', rel), 'utf8');
      expect(src, rel).toMatch(/useColumnWidths\([\s\S]*?,\s*COLUMN_WIDTHS_STORAGE_KEY\s*,?\s*\)/);
      expect(src, rel).toContain("COLUMN_WIDTHS_STORAGE_KEY = 'agenthub.");
    }
  });
});
