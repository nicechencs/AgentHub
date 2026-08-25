import { readdirSync, readFileSync, statSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { describe, expect, it } from 'vitest';

import { BUTTON } from '@/styles/tokens';

import { buttonVariants } from './button';

const srcRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../..');

const VARIANTS = [
  'default',
  'secondary',
  'outline',
  'ghost',
  'danger',
  'dangerOutline',
] as const;

const SIZES = ['default', 'sm', 'lg', 'icon'] as const;

function walkSourceFiles(dir: string, out: string[] = []): string[] {
  for (const name of readdirSync(dir)) {
    if (name === 'node_modules') continue;
    const full = path.join(dir, name);
    const st = statSync(full);
    if (st.isDirectory()) {
      walkSourceFiles(full, out);
      continue;
    }
    if (/\.(ts|tsx)$/.test(name) && !/\.(test|spec)\.(ts|tsx)$/.test(name)) {
      out.push(full);
    }
  }
  return out;
}

describe('buttonVariants (docs/ui-design.md §2 Button)', () => {
  it('locks shadow-none on rest, hover, and active for every role', () => {
    for (const variant of VARIANTS) {
      const cls = buttonVariants({ variant });
      expect(cls).toContain('shadow-none');
      expect(cls).toContain('hover:shadow-none');
      expect(cls).toContain('active:shadow-none');
      expect(cls).not.toMatch(/hover:shadow-(xs|sm|md|lg)/);
    }
  });

  it('uses only the two standard heights and 4px-ladder padding', () => {
    expect(buttonVariants({ size: 'default' })).toContain('h-7');
    expect(buttonVariants({ size: 'default' })).toContain('px-3');
    expect(buttonVariants({ size: 'sm' })).toContain('h-7');
    expect(buttonVariants({ size: 'sm' })).toContain('px-2');
    expect(buttonVariants({ size: 'sm' })).toContain('text-meta');
    expect(buttonVariants({ size: 'lg' })).toContain('h-8');
    expect(buttonVariants({ size: 'lg' })).toContain('px-4');
    expect(buttonVariants({ size: 'icon' })).toContain('h-7');
    expect(buttonVariants({ size: 'icon' })).toContain('w-7');

    for (const size of SIZES) {
      const cls = buttonVariants({ size });
      expect(cls).not.toContain('px-2.5');
      expect(cls).not.toContain('px-3.5');
    }

    expect(BUTTON.height.default).toBe(28);
    expect(BUTTON.height.lg).toBe(32);
    expect(BUTTON.padX).toEqual({ sm: 8, default: 12, lg: 16 });
    expect(BUTTON.hoverShadow).toBe('none');
  });

  it('keeps hover as fill/color, with a matching press darkening', () => {
    expect(buttonVariants({ variant: 'default' })).toContain('hover:bg-accent/90');
    expect(buttonVariants({ variant: 'default' })).toContain('active:bg-accent/80');
    expect(buttonVariants({ variant: 'secondary' })).toContain('bg-hover');
    expect(buttonVariants({ variant: 'secondary' })).not.toContain('bg-subtle');
    expect(buttonVariants({ variant: 'secondary' })).toContain('hover:bg-active');
    expect(buttonVariants({ variant: 'secondary' })).toContain('active:bg-active');
    expect(buttonVariants({ variant: 'ghost' })).toContain('hover:bg-hover');
    expect(buttonVariants({ variant: 'outline' })).toContain('hover:bg-hover');
    expect(buttonVariants({ variant: 'danger' })).toContain('hover:bg-danger/90');
    expect(buttonVariants({ variant: 'dangerOutline' })).toContain('hover:bg-danger/10');
  });
});

describe('cancel buttons', () => {
  it('uses the darker secondary fill for every common.cancel action', () => {
    const files = walkSourceFiles(srcRoot);
    const offenders: string[] = [];

    for (const file of files) {
      const text = readFileSync(file, 'utf8');
      if (!text.includes("t('common.cancel')")) continue;
      const chunks = text.split("t('common.cancel')");
      for (let i = 0; i < chunks.length - 1; i++) {
        const before = chunks[i];
        const start = before.lastIndexOf('<Button');
        if (start < 0) continue;
        const button = before.slice(start);
        if (!button.includes('variant="secondary"')) {
          offenders.push(path.relative(srcRoot, file));
        }
      }
    }

    expect(offenders).toEqual([]);
  });
});

describe('button shadow dialect', () => {
  it('does not introduce a hover-only shadow lift on Button classNames', () => {
    const files = walkSourceFiles(srcRoot);
    const hoverShadow = /hover:shadow-(xs|sm|md|lg)\b/;
    const restShadow = /(?<!hover:|active:|focus-visible:)shadow-(xs|sm|md|lg)\b/;
    const offenders: string[] = [];

    for (const file of files) {
      const text = readFileSync(file, 'utf8');
      if (!text.includes('<Button') && !text.includes('buttonVariants')) continue;
      const hover = text.match(hoverShadow);
      if (!hover) continue;
      const rest = text.match(restShadow);
      if (!rest || rest[1] !== hover[1]) {
        offenders.push(`${path.relative(srcRoot, file)}: hover shadow without matching rest shadow`);
      }
    }

    expect(offenders).toEqual([]);
  });
});
