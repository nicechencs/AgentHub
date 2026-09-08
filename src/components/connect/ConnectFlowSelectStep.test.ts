import { readFileSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { describe, expect, it } from 'vitest';

const dir = path.dirname(fileURLToPath(import.meta.url));

function source(name: string): string {
  return readFileSync(path.join(dir, name), 'utf8');
}

describe('ConnectFlowSelectStep type scale', () => {
  it('does not import Wallet and uses design-system type for group titles', () => {
    const src = source('ConnectFlowSelectStep.tsx');
    expect(src).not.toMatch(/\bWallet\b/);
    expect(src).toContain("import { RefreshCw } from 'lucide-react'");
    expect(src).toContain("t('connect.select.nativeTitle')");
    expect(src).toContain("t('connect.select.crossTitle')");
    expect(src).toMatch(/className="text-body font-medium">\{t\('connect\.select\.nativeTitle'\)\}/);
    expect(src).toMatch(/className="text-body font-medium">\{t\('connect\.select\.crossTitle'\)\}/);
    expect(src).not.toContain('text-sm font-medium');
  });
});
