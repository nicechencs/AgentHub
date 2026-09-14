import { describe, expect, it } from 'vitest';
import {
  CHAT_CONTENT_WIDTH_ADAPTIVE_CAP,
  CHAT_CONTENT_WIDTH_ADAPTIVE_FLOOR,
  CHAT_CONTENT_WIDTH_MIN,
  resolveChatContentWidth,
} from './chat-content-width';
import {
  isPreviewableChatFilePath,
  parseChatFileLineRef,
  stripChatFileLineSuffix,
} from './chat-file-preview';
import {
  chatPreviewLine,
  openChatPreviewRoot,
  pushChatPreview,
} from './chat-preview-model';

describe('chat content width', () => {
  it('uses adaptive clamp when no preference is stored', () => {
    expect(resolveChatContentWidth(2000, null)).toBe(CHAT_CONTENT_WIDTH_ADAPTIVE_CAP);
    expect(resolveChatContentWidth(1000, null)).toBe(CHAT_CONTENT_WIDTH_ADAPTIVE_FLOOR);
    expect(resolveChatContentWidth(1200, null)).toBe(768);
  });

  it('clamps a dragged preference without going below the floor', () => {
    expect(resolveChatContentWidth(2000, 500)).toBe(CHAT_CONTENT_WIDTH_MIN);
    expect(resolveChatContentWidth(2000, 970)).toBe(970);
    expect(resolveChatContentWidth(800, 970)).toBe(Math.max(CHAT_CONTENT_WIDTH_MIN, 800 - 176));
  });
});

describe('chat file preview targets', () => {
  it('accepts common text and source extensions', () => {
    expect(isPreviewableChatFilePath('README.md')).toBe(true);
    expect(isPreviewableChatFilePath('src/app.ts')).toBe(true);
    expect(isPreviewableChatFilePath('config.json')).toBe(true);
    expect(isPreviewableChatFilePath('photo.png')).toBe(false);
  });

  it('parses GitHub-style and colon line refs', () => {
    expect(parseChatFileLineRef('a.ts#L12')).toBe(12);
    expect(parseChatFileLineRef('a.ts#L12-L20')).toBe(12);
    expect(parseChatFileLineRef('a.ts#line=9')).toBe(9);
    expect(parseChatFileLineRef('a.ts:42')).toBe(42);
    expect(parseChatFileLineRef('C:\\repo\\a.ts:7')).toBe(7);
    expect(stripChatFileLineSuffix('a.ts:42')).toBe('a.ts');
    expect(stripChatFileLineSuffix('C:\\repo\\a.ts:7')).toBe('C:\\repo\\a.ts');
  });

  it('keeps the line on the preview target', () => {
    const root = openChatPreviewRoot('/repo/a.ts', 12);
    expect(chatPreviewLine(root)).toBe(12);
    const same = pushChatPreview(root, '/repo/a.ts', 20);
    expect(chatPreviewLine(same)).toBe(20);
  });
});
