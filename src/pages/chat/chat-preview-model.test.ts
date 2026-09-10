import { describe, expect, it } from 'vitest';
import {
  chatPreviewCanBack,
  chatPreviewPath,
  isChatFilePreview,
  isChatProcessInspect,
  openChatPreviewRoot,
  openChatProcessInspect,
  popChatPreview,
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
    const process = openChatProcessInspect(2, 'codex');
    expect(isChatProcessInspect(process)).toBe(true);
    expect(isChatFilePreview(process)).toBe(false);
    expect(chatPreviewPath(process)).toBe('');
    expect(chatPreviewCanBack(process)).toBe(false);
    expect(popChatPreview(process)).toBeNull();
    expect(pushChatPreview(process, '/repo/README.md')).toEqual(
      openChatPreviewRoot('/repo/README.md'),
    );
  });
});
