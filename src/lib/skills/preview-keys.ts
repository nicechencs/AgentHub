import type { AgentKey } from '@/lib/types';

/** Stable identity for the open SKILL.md preview (shared vs private vs project). */
export type SkillPreviewTargetLike = {
  skillId: string;
  privateAgent?: AgentKey | null;
  workspacePath?: string | null;
  originRoot?: string | null;
};

/**
 * Composite key that distinguishes shared vs agent-private vs project skills with the same id.
 * - shared: `shared:${skillId}`
 * - private: `agent:${privateAgent}:${skillId}`
 * - project: `project:${workspacePath}:${origin}:${skillId}`
 */
export function skillPreviewActiveKey(target: SkillPreviewTargetLike): string {
  if (target.workspacePath) {
    const origin = target.originRoot?.trim() || '.agents/skills';
    return `project:${target.workspacePath}:${origin}:${target.skillId}`;
  }
  const agent = target.privateAgent;
  if (agent) return `agent:${agent}:${target.skillId}`;
  return `shared:${target.skillId}`;
}

export function sharedSkillActiveKey(skillId: string): string {
  return skillPreviewActiveKey({ skillId });
}

export function privateSkillActiveKey(agentId: AgentKey, skillId: string): string {
  return skillPreviewActiveKey({ skillId, privateAgent: agentId });
}

/** True when a higher-priority overlay should consume Esc before the preview pane. */
export function hasEscPriorityOverlay(root: ParentNode = document): boolean {
  if (root.querySelector('[role="dialog"][data-state="open"]')) return true;
  if (root.querySelector('[role="alertdialog"][data-state="open"]')) return true;
  if (root.querySelector('[role="menu"][data-state="open"]')) return true;
  if (root.querySelector('[role="listbox"][data-state="open"]')) return true;
  if (root.querySelector('[data-radix-menu-content][data-state="open"]')) return true;
  if (root.querySelector('[data-radix-popper-content-wrapper] [data-state="open"]')) return true;
  return false;
}

/** Whether ArrowUp/Down / list keyboard should be ignored (typing / open menus). */
export function shouldIgnoreListKeyboard(target: EventTarget | null): boolean {
  if (!target || typeof target !== 'object') return false;
  // Duck-typed for browser + node unit tests (no HTMLElement global in vitest/node).
  const el = target as {
    tagName?: string;
    isContentEditable?: boolean;
    closest?: (selectors: string) => unknown;
  };
  const tag = el.tagName;
  if (tag === 'INPUT' || tag === 'TEXTAREA' || tag === 'SELECT') return true;
  if (el.isContentEditable) return true;
  if (
    typeof el.closest === 'function' &&
    el.closest('[role="menu"], [role="listbox"], [role="dialog"], [role="alertdialog"]')
  ) {
    return true;
  }
  return false;
}
