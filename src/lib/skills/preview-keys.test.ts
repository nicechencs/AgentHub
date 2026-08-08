import { describe, expect, it } from 'vitest';
import {
  hasEscPriorityOverlay,
  privateSkillActiveKey,
  sharedSkillActiveKey,
  shouldIgnoreListKeyboard,
  skillPreviewActiveKey,
} from './preview-keys';

function mockRoot(hits: string[]): ParentNode {
  const set = new Set(hits);
  return {
    querySelector(sel: string) {
      return set.has(sel) ? ({} as Element) : null;
    },
  } as ParentNode;
}

describe('skillPreviewActiveKey', () => {
  it('distinguishes shared vs private with the same skill id', () => {
    const shared = skillPreviewActiveKey({ skillId: 'dbs-action' });
    const privateCodex = skillPreviewActiveKey({
      skillId: 'dbs-action',
      privateAgent: 'codex',
    });
    const privateClaude = skillPreviewActiveKey({
      skillId: 'dbs-action',
      privateAgent: 'claude',
    });

    expect(shared).toBe('shared:dbs-action');
    expect(privateCodex).toBe('agent:codex:dbs-action');
    expect(privateClaude).toBe('agent:claude:dbs-action');
    expect(shared).not.toBe(privateCodex);
    expect(privateCodex).not.toBe(privateClaude);
  });

  it('treats null/undefined privateAgent as shared', () => {
    expect(skillPreviewActiveKey({ skillId: 'x', privateAgent: null })).toBe('shared:x');
    expect(skillPreviewActiveKey({ skillId: 'x', privateAgent: undefined })).toBe('shared:x');
    expect(sharedSkillActiveKey('x')).toBe('shared:x');
    expect(privateSkillActiveKey('grok', 'x')).toBe('agent:grok:x');
  });

  it('never uses sourceDir as identity (only skillId + privateAgent)', () => {
    const a = skillPreviewActiveKey({ skillId: 'same' });
    const b = skillPreviewActiveKey({ skillId: 'same', privateAgent: null });
    expect(a).toBe(b);
    expect(privateSkillActiveKey('codex', 'same')).not.toBe(a);
  });
});

describe('hasEscPriorityOverlay', () => {
  it('returns false when no overlay matches', () => {
    expect(hasEscPriorityOverlay(mockRoot([]))).toBe(false);
  });

  it('detects open dialog so Esc closes dialog first', () => {
    expect(hasEscPriorityOverlay(mockRoot(['[role="dialog"][data-state="open"]']))).toBe(true);
  });

  it('detects open menu', () => {
    expect(hasEscPriorityOverlay(mockRoot(['[role="menu"][data-state="open"]']))).toBe(true);
  });
});

describe('shouldIgnoreListKeyboard', () => {
  it('ignores null / non-elements', () => {
    expect(shouldIgnoreListKeyboard(null)).toBe(false);
    expect(shouldIgnoreListKeyboard({} as EventTarget)).toBe(false);
  });

  it('ignores input-like targets via tagName', () => {
    const input = { tagName: 'INPUT', isContentEditable: false, closest: () => null };
    const textarea = { tagName: 'TEXTAREA', isContentEditable: false, closest: () => null };
    const div = { tagName: 'DIV', isContentEditable: false, closest: () => null };
    expect(shouldIgnoreListKeyboard(input as unknown as EventTarget)).toBe(true);
    expect(shouldIgnoreListKeyboard(textarea as unknown as EventTarget)).toBe(true);
    expect(shouldIgnoreListKeyboard(div as unknown as EventTarget)).toBe(false);
  });

  it('ignores targets inside dialog via closest', () => {
    const btn = {
      tagName: 'BUTTON',
      isContentEditable: false,
      closest: (sel: string) => (sel.includes('dialog') ? {} : null),
    };
    expect(shouldIgnoreListKeyboard(btn as unknown as EventTarget)).toBe(true);
  });
});
