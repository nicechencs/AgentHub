import { readFileSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { createElement } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it } from 'vitest';
import { SortHandle } from './SortHandle';

const dir = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(dir, '../../..');

function source(rel: string): string {
  return readFileSync(path.join(dir, rel), 'utf8');
}

describe('sortable drag wiring', () => {
  it('uses pointer hit-testing instead of HTML5 drag events', () => {
    const hook = source('use-sortable-drag.ts');
    expect(hook).toContain('elementFromPoint');
    expect(hook).toContain('pointermove');
    expect(hook).toContain('pointerup');
    expect(hook).toContain('SORTABLE_ID_ATTR');
    expect(hook).toContain('cloneNode');
    expect(hook).toContain('SORTABLE_PREVIEW_ATTR');
    expect(hook).toContain('previewTransform');
    expect(hook).not.toContain('onDragOver');
    expect(hook).not.toContain('onDrop');
    expect(hook).not.toContain('dataTransfer');
  });

  it('starts a drag from the handle without a native draggable node', () => {
    const handle = source('SortHandle.tsx');
    expect(handle).toContain('onPointerDown');
    expect(handle).toContain('pointer-events-none');
    expect(handle).toContain('touch-none');
    expect(handle).not.toContain('draggable');
    expect(handle).not.toContain('onDragStart=');
    expect(handle).not.toContain('dataTransfer');
  });

  it('renders a keyboard-accessible grip that ignores icon hit-testing', () => {
    const html = renderToStaticMarkup(
      createElement(SortHandle, { id: 'row-1', onDragStartId() {} }),
    );
    expect(html).toContain('拖动排序');
    expect(html).toContain('pointer-events-none');
    expect(html).toContain('role="button"');
    expect(html).not.toContain('draggable="true"');
  });

  it('turns off Tauri HTML5 file-drop interception on the main window', () => {
    const conf = JSON.parse(
      readFileSync(path.join(repoRoot, 'src-tauri/tauri.conf.json'), 'utf8'),
    ) as { app: { windows: Array<{ dragDropEnabled?: boolean }> } };
    expect(conf.app.windows[0]?.dragDropEnabled).toBe(false);
  });
});
