/**
 * Kiro Chat honesty: one-shot headless, not ChatRuntime.
 * Catalog may not include `kiro` yet — helpers key off the id only.
 */
import type { TranslateFn } from '@/lib/i18n';
import { isRuntimeChatAgent } from './chat-runtime-model';

export function isKiroChatAgent(agentId: string | null | undefined): boolean {
  return agentId === 'kiro';
}

/** Cursor-style half-surface: fire-and-forget CLI, not a continuous session. */
export function isOneShotHeadlessChatAgent(agentId: string | null | undefined): boolean {
  return agentId === 'kiro' || agentId === 'cursor';
}

export type KiroChatStance = {
  showBanner: true;
  allowCommandSearch: false;
  allowRuntimeRequests: false;
  allowModelPicker: false;
  allowSteer: false;
  allowQueueFollowUp: false;
};

/** Product stance when the conversation Agent is Kiro. Other ids return null. */
export function kiroChatStance(agentId: string | null | undefined): KiroChatStance | null {
  if (!isKiroChatAgent(agentId)) return null;
  return {
    showBanner: true,
    allowCommandSearch: false,
    allowRuntimeRequests: false,
    allowModelPicker: false,
    allowSteer: false,
    allowQueueFollowUp: false,
  };
}

export function kiroChatAllowsCommandSearch(agentId: string | null | undefined): boolean {
  return kiroChatStance(agentId)?.allowCommandSearch !== false;
}

export function chatShowsRuntimeRequestPanels(agentId: string | null | undefined): boolean {
  return isRuntimeChatAgent(agentId);
}

/** Headless Kiro has no `/model` picker; leftover lists must not appear. */
export function chatComposerChoiceOptions(
  agentId: string | null | undefined,
  options: readonly string[],
): string[] {
  if (kiroChatStance(agentId)?.allowModelPicker === false) return [];
  return [...options];
}

export function kiroChatBannerCopy(t: TranslateFn): {
  title: string;
  detail: string;
} {
  return {
    title: t('chat.kiro.oneshotHint'),
    detail: t('chat.kiro.oneshotDetail'),
  };
}

export function kiroChatComposerPlaceholder(
  t: TranslateFn,
  agentId: string | null | undefined,
  fallback: string,
): string {
  return isKiroChatAgent(agentId) ? t('chat.kiro.placeholder') : fallback;
}
