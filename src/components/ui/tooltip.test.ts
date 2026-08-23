/**
 * Tooltip 气泡规格：宽高/换行/字体/底色/圆角锁在 token + TOOLTIP_SURFACE_CLASS。
 */
import { readFileSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { describe, expect, it } from 'vitest';
import { TOOLTIP } from '@/styles/tokens';
import { TOOLTIP_SURFACE_CLASS, tooltipSurfaceStyle } from './tooltip';

const here = path.dirname(fileURLToPath(import.meta.url));
const srcRoot = path.resolve(here, '../..');

describe('tooltip surface contract', () => {
  it('locks wrap width, viewport cap, type, and bubble chrome', () => {
    expect(TOOLTIP).toEqual({
      maxWidth: '280px',
      maxHeight: '192px',
      paddingX: '10px',
      paddingY: '6px',
      sideOffset: 8,
      collisionPadding: 8,
      delayMs: 200,
    });
    expect(TOOLTIP_SURFACE_CLASS).toContain('max-w-[var(--tooltip-max-width)]');
    expect(TOOLTIP_SURFACE_CLASS).toContain('max-h-[min(var(--tooltip-max-height),calc(100vh-16px))]');
    expect(TOOLTIP_SURFACE_CLASS).toContain('w-max');
    expect(TOOLTIP_SURFACE_CLASS).toContain('break-words');
    expect(TOOLTIP_SURFACE_CLASS).toContain('[overflow-wrap:anywhere]');
    expect(TOOLTIP_SURFACE_CLASS).toContain('rounded-card');
    expect(TOOLTIP_SURFACE_CLASS).toContain('bg-panel');
    expect(TOOLTIP_SURFACE_CLASS).toContain('border-border');
    expect(TOOLTIP_SURFACE_CLASS).toContain('text-meta');
    expect(TOOLTIP_SURFACE_CLASS).toContain('font-sans');
    expect(TOOLTIP_SURFACE_CLASS).toContain('shadow-sm');
    expect(TOOLTIP_SURFACE_CLASS).not.toContain('max-w-xs');
    expect(TOOLTIP_SURFACE_CLASS).not.toContain('max-w-sm');
  });

  it('chart/surface helper reuses the same geometry and colors', () => {
    expect(tooltipSurfaceStyle()).toMatchObject({
      backgroundColor: 'var(--bg-panel)',
      border: '1px solid var(--border)',
      borderRadius: 'var(--radius)',
      maxWidth: TOOLTIP.maxWidth,
      maxHeight: `min(${TOOLTIP.maxHeight}, calc(100vh - 16px))`,
      fontSize: 'var(--font-meta-size)',
      padding: `${TOOLTIP.paddingY} ${TOOLTIP.paddingX}`,
      whiteSpace: 'normal',
    });
  });

  it('Hint contentClassName cannot override bubble chrome; delay uses TOOLTIP', () => {
    const src = readFileSync(path.join(here, 'tooltip.tsx'), 'utf8');
    expect(src).toContain('TooltipBody contentClassName={contentClassName}');
    expect(src).not.toContain('className={contentClassName}');
    expect(src).toContain('cn(className, TOOLTIP_SURFACE_CLASS)');
    expect(src).not.toContain('disableHoverableContent');
    expect(readFileSync(path.join(srcRoot, 'main.tsx'), 'utf8')).toContain(
      'delayDuration={TOOLTIP.delayMs}',
    );
  });

  it('EnvStatusBar structured hint only sets inner stacking, not width', () => {
    const src = readFileSync(path.join(srcRoot, 'components/shared/EnvStatusBar.tsx'), 'utf8');
    expect(src).toContain('contentClassName="space-y-0.5"');
    expect(src).not.toMatch(/contentClassName="[^"]*max-w-/);
  });
});
