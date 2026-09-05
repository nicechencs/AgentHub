import { readFileSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { describe, expect, it } from 'vitest';

const dir = path.dirname(fileURLToPath(import.meta.url));

function source(rel: string): string {
  return readFileSync(path.join(dir, rel), 'utf8');
}

describe('PageHelpButton', () => {
  it('keeps the first-run chrome hint from opening the tutorial', () => {
    const hint = source('ChromeHint.tsx');
    expect(hint).toContain('createPortal');
    expect(hint).toContain('chrome.hint.help');
    expect(hint).toContain('chrome.hint.feedback');
    expect(hint).not.toContain('setOpen(true)');
    expect(hint).not.toContain('PageHelpButton');
  });

  it('shows the tutorial as a bubble tour, not a dialog list', () => {
    const tour = source('PageHelpTour.tsx');
    expect(tour).toContain('createPortal');
    expect(tour).toContain('pickHelpTargetRect');
    expect(tour).toContain('chrome.pageHelp.next');
    expect(tour).not.toContain('DialogContent');
    expect(tour).not.toContain('list-decimal');
  });

  it('opens the current-page tutorial only after a click', () => {
    const button = source('PageHelpButton.tsx');
    expect(button).toContain('useState(false)');
    expect(button).toContain('setOpen(true)');
    expect(button).toContain('CircleHelp');
    expect(button).toContain('chrome.pageHelp.label');
    expect(button).toContain('pageHelpIdFromPath');
    expect(button).toContain('pathname, search');
    expect(button).toContain('dismissChromeHint');
    expect(button).toContain('PageHelpTour');
    expect(button).not.toContain('DialogContent');
    expect(button).not.toContain('useState(true)');
  });
});
