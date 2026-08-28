/**
 * Ticket add menu and dialog-open helpers (Connections page).
 */
import { agentDisplayName } from '@/config/agents';
import type { AgentId } from '@/lib/types';
import type { TranslateFn } from '@/lib/i18n';
import {
  MENU_DIALOG_DISMISS_CLEAR_MS,
  scheduleAfterMenuClose,
} from '@/lib/menu-dialog-arm';

export {
  armMenuDialogOpen,
  handleMenuDialogSelect,
  MENU_DIALOG_DISMISS_CLEAR_MS,
  scheduleAfterMenuClose,
  shouldIgnoreMenuDialogDismiss,
} from '@/lib/menu-dialog-arm';

export type TicketAddKind = 'import-login' | 'oauth' | 'api-key';

export const TICKET_ADD_ACTIONS: Array<{ kind: TicketAddKind; label: string }> = [
  { kind: 'import-login', label: '导入授权' },
  { kind: 'oauth', label: '官方登录' },
  { kind: 'api-key', label: '添加 API Key' },
];

export function ticketAddActionsForAgent(
  oauthLogin = false,
): Array<{ kind: TicketAddKind; label: string }> {
  if (oauthLogin) return TICKET_ADD_ACTIONS;
  return TICKET_ADD_ACTIONS.filter((item) => item.kind !== 'oauth');
}

export function ticketAddActionLabel(kind: TicketAddKind, t?: TranslateFn): string {
  if (!t) {
    return TICKET_ADD_ACTIONS.find((item) => item.kind === kind)?.label ?? '导入授权';
  }
  if (kind === 'import-login') return t('connections.list.importLogin');
  if (kind === 'oauth') return t('connections.list.addOauth');
  return t('connections.list.addApiKey');
}

export interface TicketAddMenuAgent {
  id: AgentId;
  name: string;
  actions: Array<{ kind: TicketAddKind; label: string }>;
}

function oauthLoginSet(
  oauthLoginAgents?: ReadonlySet<string> | readonly string[] | null,
): ReadonlySet<string> {
  if (!oauthLoginAgents) return new Set();
  return oauthLoginAgents instanceof Set ? oauthLoginAgents : new Set(oauthLoginAgents);
}

export function buildTicketAddMenu(
  agentIds?: readonly AgentId[] | null,
  oauthLoginAgents?: ReadonlySet<string> | readonly string[] | null,
): TicketAddMenuAgent[] {
  if (!agentIds || agentIds.length === 0) return [];
  const oauth = oauthLoginSet(oauthLoginAgents);
  return agentIds.map((id) => ({
    id,
    name: agentDisplayName(id),
    actions: ticketAddActionsForAgent(oauth.has(id)),
  }));
}

/** When an Agent tab is selected, skip the agent picker and use that Agent's actions. */
export function focusedTicketAddAgent(
  agents: readonly TicketAddMenuAgent[],
  focusedAgentId?: AgentId | null,
): TicketAddMenuAgent | null {
  if (!focusedAgentId) return null;
  return agents.find((item) => item.id === focusedAgentId) ?? null;
}

export function dispatchTicketAddAction(
  kind: TicketAddKind,
  agentId: AgentId,
  handlers: {
    onImportLogin?: (id: AgentId) => void;
    onOauth?: (id: AgentId) => void;
    onAddKey?: (id: AgentId) => void;
  },
): void {
  if (kind === 'import-login') {
    handlers.onImportLogin?.(agentId);
    return;
  }
  if (kind === 'oauth') {
    handlers.onOauth?.(agentId);
    return;
  }
  handlers.onAddKey?.(agentId);
}

/**
 * Menu item select for 导入授权 / 添加 API Key.
 * preventDefault keeps the menu mounted through the click so the Dialog is
 * not dismissed and the pointer cannot hit the segmented filter underneath.
 * Close is delayed until after the click settles — timeout 0 unmounts the
 * submenu in time for the same click to hit AgentTabStrip (silence).
 */
/** Expanded 添加授权 stays open after click-to-expand; Escape still closes it. */
export function ticketAddMenuClosesOnKey(key: string): boolean {
  return key === 'Escape' || key === 'Esc';
}

export function handleTicketAddMenuSelect(
  event: { preventDefault: () => void; stopPropagation?: () => void },
  kind: TicketAddKind,
  agentId: AgentId,
  handlers: {
    onImportLogin?: (id: AgentId) => void;
    onOauth?: (id: AgentId) => void;
    onAddKey?: (id: AgentId) => void;
    onMenuClose?: () => void;
  },
  schedule: (fn: () => void, delayMs?: number) => void = scheduleAfterMenuClose,
  closeDelayMs = MENU_DIALOG_DISMISS_CLEAR_MS,
): void {
  event.preventDefault();
  event.stopPropagation?.();
  dispatchTicketAddAction(kind, agentId, handlers);
  if (handlers.onMenuClose) schedule(handlers.onMenuClose, closeDelayMs);
}

export function ticketAddDialogState(
  kind: TicketAddKind,
  agentId: AgentId,
): {
  addAgentId: AgentId;
  loginImportOpen: boolean;
  oauthDialogOpen: boolean;
  apiKeyDialogOpen: boolean;
  clearEditProvider: boolean;
} {
  return {
    addAgentId: agentId,
    loginImportOpen: kind === 'import-login',
    oauthDialogOpen: kind === 'oauth',
    apiKeyDialogOpen: kind === 'api-key',
    clearEditProvider: kind === 'api-key',
  };
}
