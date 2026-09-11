/**
 * Kiro Chat: new conversations use ACP (`kiro-cli acp`), same class as Grok.
 * Allow/deny and queue-after-turn are real. Mid-turn steer is not.
 * Legacy print chats stay off ChatRuntime until the user continues.
 */
import type { TranslateFn } from '@/lib/i18n';

export function isKiroChatAgent(agentId: string | null | undefined): boolean {
  return agentId === 'kiro';
}

/** Per-turn CLI send/wait (no mid-run steer); Cursor remains on this list. */
export function isOneShotHeadlessChatAgent(agentId: string | null | undefined): boolean {
  return agentId === 'cursor';
}

export type KiroChatStance = {
  showBanner: boolean;
  allowCommandSearch: boolean;
  allowRuntimeRequests: boolean;
  allowModelPicker: boolean;
  allowSteer: boolean;
  allowQueueFollowUp: boolean;
};

/** Kiro no longer uses a one-shot banner; continuous ACP follows Grok gates. */
export function kiroChatStance(_agentId: string | null | undefined): KiroChatStance | null {
  return null;
}

export function kiroChatAllowsCommandSearch(agentId: string | null | undefined): boolean {
  return kiroChatStance(agentId)?.allowCommandSearch !== false;
}

/** Confirmation cards follow the bound snapshot, not the Agent name whitelist. */
export function chatShowsRuntimeRequestPanels(runtimeEnabled: boolean | undefined): boolean {
  return runtimeEnabled === true;
}

/** Composer model/effort chips follow `allowModelPicker` (enabled for Kiro). */
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

/** Kiro uses the shared invite placeholder; limits go to `composerCapabilityHint`. */
export function kiroChatComposerPlaceholder(
  _t: TranslateFn,
  _agentId: string | null | undefined,
  fallback: string,
): string {
  return fallback;
}
