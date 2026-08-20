import { readFileSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { describe, expect, it } from 'vitest';

const dir = path.dirname(fileURLToPath(import.meta.url));

function source(name: string): string {
  return readFileSync(path.join(dir, name), 'utf8');
}

describe('chat layout wiring', () => {
  it('keeps the main column on canvas so an empty transcript matches composer chrome', () => {
    const page = source('index.tsx');
    expect(page).toContain('flex min-w-0 flex-1 flex-col bg-canvas');
    expect(page).toContain('chatComposerChromeClass(page.turns.length > 0)');
    expect(page).not.toContain('flex min-w-0 flex-1 flex-col bg-panel');
  });

  it('grows the composer textarea with a shared cap and panel-colored shell', () => {
    const composer = source('ChatComposer.tsx');
    expect(composer).toContain('[field-sizing:content]');
    expect(composer).toContain('COMPOSER_TEXTAREA_MIN_PX');
    expect(composer).toContain('COMPOSER_TEXTAREA_MAX_PX');
    expect(composer).toContain('composerTextareaMeasuredStyle');
    expect(composer).toContain('composerUsesCssFieldSizing');
    expect(composer).toContain('rounded-composer border border-border bg-panel');
  });

  it('derives the transcript surface from whether any turns exist', () => {
    expect(source('ChatTranscript.tsx')).toContain(
      'chatTranscriptSurfaceClass(turns.length > 0)',
    );
  });
});
