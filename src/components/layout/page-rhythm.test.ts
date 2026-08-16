import { describe, expect, it } from 'vitest';
import { pageRhythm } from '@/components/layout/page-rhythm';

describe('pageRhythm (docs/ui-design.md §2)', () => {
  it('keeps page shell at 24px inset and 1200 content width', () => {
    expect(pageRhythm.pageShell).toContain('max-w-content');
    expect(pageRhythm.pageShell).toContain('px-6');
    expect(pageRhythm.pageShell).toContain('py-6');
  });

  it('keeps Chat chrome at 16px and Skills workbench at 24px', () => {
    expect(pageRhythm.chatChromeX).toBe('px-4');
    expect(pageRhythm.workbenchX).toBe('px-6');
  });

  it('separates page sections from nav eyebrows', () => {
    expect(pageRhythm.section).toBe('mt-6');
    expect(pageRhythm.sectionEyebrow).toContain('text-2xs');
    expect(pageRhythm.sectionEyebrow).toContain('uppercase');
    expect(pageRhythm.sectionEyebrow).not.toContain('text-base');
  });
});
