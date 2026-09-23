import { describe, expect, it } from 'vitest';
import {
  chatPreviewCanBack,
  chatPreviewLine,
  chatPreviewPath,
  isChatEditPreview,
  isChatFilePreview,
  isChatProcessInspect,
  openChatEditPreview,
  openChatPreviewRoot,
  openChatProcessInspect,
  popChatPreview,
  processInspectRowAction,
  processInspectStepIndex,
  processInspectStepKey,
  pushChatPreview,
} from './chat-preview-model';

describe('chat preview stack', () => {
  it('opens a root file without a back step', () => {
    const target = openChatPreviewRoot('/repo/README.md');
    expect(chatPreviewPath(target)).toBe('/repo/README.md');
    expect(chatPreviewCanBack(target)).toBe(false);
  });

  it('pushes a nested file so the previous one can be restored', () => {
    const root = openChatPreviewRoot('/repo/README.md');
    const nested = pushChatPreview(root, '/repo/docs/guide.md');
    expect(chatPreviewPath(nested)).toBe('/repo/docs/guide.md');
    expect(chatPreviewCanBack(nested)).toBe(true);
    expect(popChatPreview(nested)).toEqual(root);
  });

  it('ignores a push to the file already on screen', () => {
    const root = openChatPreviewRoot('/repo/README.md');
    expect(pushChatPreview(root, '/repo/README.md')).toEqual(root);
  });

  it('clears the preview when popping the last file', () => {
    expect(popChatPreview(openChatPreviewRoot('/repo/README.md'))).toBeNull();
  });

  it('opens process inspect separately from the file stack', () => {
    const process = openChatProcessInspect(2, 'codex', 'step:1');
    expect(process.stepKey).toBe('step:1');
    expect(isChatProcessInspect(process)).toBe(true);
    expect(isChatFilePreview(process)).toBe(false);
    expect(chatPreviewPath(process)).toBe('');
    expect(chatPreviewCanBack(process)).toBe(false);
    expect(popChatPreview(process)).toBeNull();
    expect(pushChatPreview(process, '/repo/README.md')).toEqual(
      openChatPreviewRoot('/repo/README.md'),
    );
  });

  it('keeps a 1-based line only on file previews', () => {
    expect(chatPreviewLine(openChatPreviewRoot('/repo/README.md', 12))).toBe(12);
    expect(chatPreviewLine(openChatPreviewRoot('/repo/README.md', 0))).toBeUndefined();
    expect(chatPreviewLine(openChatPreviewRoot('/repo/README.md', -1))).toBeUndefined();
    expect(chatPreviewLine(openChatEditPreview('src/a.ts'))).toBeUndefined();
    expect(chatPreviewLine(null)).toBeUndefined();
    const same = pushChatPreview(openChatPreviewRoot('/repo/README.md', 3), '/repo/README.md', 9);
    expect(chatPreviewLine(same)).toBe(9);
    expect(chatPreviewPath(same)).toBe('/repo/README.md');
    expect(pushChatPreview(null, '/repo/README.md', 4)).toEqual(
      openChatPreviewRoot('/repo/README.md', 4),
    );
  });

  it('opens an edit preview by path without a back stack', () => {
    const target = openChatEditPreview('src/a.ts');
    expect(isChatEditPreview(target)).toBe(true);
    expect(isChatFilePreview(target)).toBe(false);
    expect(chatPreviewPath(target)).toBe('src/a.ts');
    expect(chatPreviewCanBack(target)).toBe(false);
    expect(popChatPreview(target)).toBeNull();
    expect(pushChatPreview(target, '/repo/README.md')).toEqual(
      openChatPreviewRoot('/repo/README.md'),
    );
  });

  it('keeps the detail open when moving to another step and closes only the current one', () => {
    expect(processInspectStepKey(0)).toBe('step:0');
    expect(processInspectStepIndex('step:1')).toBe(1);
    expect(processInspectStepIndex('generating')).toBeNull();
    expect(processInspectRowAction(true, 'step:0', 'step:1')).toBe('focus');
    expect(processInspectRowAction(true, 'step:0', 'step:0')).toBe('close');
    expect(processInspectRowAction(false, 'step:0', 'step:0')).toBe('focus');
    expect(processInspectRowAction(true, null, 'step:0')).toBe('focus');
  });

  it('keeps the clicked turn on an edit preview', () => {
    expect(openChatEditPreview('src/a.ts', 2)).toEqual({ kind: 'edit', path: 'src/a.ts', turn: 2 });
    expect(openChatEditPreview('src/a.ts')).toEqual({ kind: 'edit', path: 'src/a.ts' });
  });
});
