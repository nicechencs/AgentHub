/**
 * Shared credential-row read model (P2-7).
 * Connections / ConnectFlow / ticket wallet project from this axis.
 * Do not import this from a page module into contracts; pages consume via lib façades.
 */
import { looksLikeOfficialEndpoint } from '@/config/official-api';
import {
  extractProviderEndpoint,
  formatEndpointHost,
  formatLocalRouteLabel,
  isInternalGeneratedName,
  isInternalGeneratedProvider,
} from '@/lib/backend/contracts/agent-connection';
import {
  authDisplayForAccount,
  authHealthLabel,
  type AuthHealth,
} from '@/lib/backend/contracts/auth-state';
import {
  ticketCredentialClassLabel,
  ticketSurfaceLabel,
  type TicketView,
} from '@/lib/backend/contracts/ticket';
import type { TranslateFn } from '@/lib/i18n';
import type { Account, AgentId, AuthStatus, Provider } from '@/lib/types';

/** Stable auth summary shared by list / picker / wallet projections. */
export type CredentialRowAuth = {
  status: AuthStatus;
  health?: AuthHealth;
  label: string;
};

/**
 * Canonical credential list row.
 * Callers add UI-only fields (usage, viaAdapter, group, highlighted, …).
 */
export type CredentialRow = {
  /** `account:<id>` / `provider:<id>` (ticket.id is already this shape). */
  key: string;
  source: 'account' | 'provider';
  id: string;
  agentId: AgentId;
  title: string;
  subtitle: string;
  isCurrent: boolean;
  auth: CredentialRowAuth;
};

export type CredentialRowInput =
  | { source: 'account'; account: Account }
  | { source: 'provider'; provider: Provider }
  | {
      source: 'ticket';
      ticket: TicketView;
      /** Pool/binding current flag when known; tickets alone have no isCurrent. */
      isCurrent?: boolean;
    };

function accountSubtitle(a: Account, t?: TranslateFn): string {
  if (a.isCurrent) {
    const bits: string[] = [];
    bits.push(authDisplayForAccount(a).label);
    if (a.subscription) bits.push(a.subscription);
    return bits.join(' · ');
  }
  const bits: string[] = [];
  bits.push(authDisplayForAccount(a).label, t ? t('connections.list.notCurrent') : '未生效');
  if (a.provider && !a.label.includes(a.provider)) bits.push(a.provider);
  if (a.subscription) bits.push(a.subscription);
  return bits.join(' · ');
}

/** Resolve official vs custom endpoint mode for a Provider row. */
export function providerEndpointMode(
  p: Provider,
  endpoint?: string,
): 'official' | 'custom' {
  if (p.official === true) return 'official';
  if (p.official === false) return 'custom';
  if (p.preset && /anthropic|openai|moonshot|xai/i.test(p.preset) && !/compat|custom|relay/i.test(p.preset)) {
    if (!endpoint || looksLikeOfficialEndpoint(p.agentId, endpoint)) return 'official';
  }
  if (!endpoint || looksLikeOfficialEndpoint(p.agentId, endpoint)) return 'official';
  return 'custom';
}

function providerSubtitle(
  p: Provider,
  endpoint: string | undefined,
  mode: 'official' | 'custom',
  t?: TranslateFn,
): string {
  const modeLabel = mode === 'official'
    ? (t ? t('connections.list.officialEndpoint') : '官方端点')
    : (t ? t('connections.list.customEndpoint') : '自定义端点');
  const host = endpoint ? formatEndpointHost(endpoint) : undefined;
  if (p.isCurrent) {
    return host
      ? (t ? t('connections.list.configuredCurrentHost', { mode: modeLabel, host }) : `已配置 · 当前生效 · ${modeLabel} · ${host}`)
      : (t ? t('connections.list.configuredCurrent', { mode: modeLabel }) : `已配置 · 当前生效 · ${modeLabel}`);
  }
  return host
    ? (t ? t('connections.list.configuredIdleHost', { mode: modeLabel, host }) : `已配置 · 未生效 · ${modeLabel} · ${host}`)
    : (t ? t('connections.list.configuredIdle', { mode: modeLabel }) : `已配置 · 未生效 · ${modeLabel}`);
}

function ticketSubtitle(ticket: TicketView, t?: TranslateFn): string {
  const classLabel = ticketCredentialClassLabel(ticket.credentialClass, t);
  const surface = ticketSurfaceLabel(ticket.surface, t);
  if (surface && surface !== classLabel) {
    return `${classLabel} · ${surface}`;
  }
  return classLabel;
}

function fromAccount(account: Account, t?: TranslateFn): CredentialRow {
  const display = authDisplayForAccount(account);
  return {
    key: `account:${account.id}`,
    source: 'account',
    id: account.id,
    agentId: account.agentId,
    title: account.label,
    subtitle: accountSubtitle(account, t),
    isCurrent: account.isCurrent,
    auth: {
      status: display.legacyStatus,
      health: display.health,
      label: display.label,
    },
  };
}

function fromProvider(provider: Provider, t?: TranslateFn): CredentialRow {
  const endpoint = extractProviderEndpoint(provider.configText, provider.configFormat);
  const internal = isInternalGeneratedProvider(provider);
  const mode = providerEndpointMode(provider, endpoint);
  const title = internal ? formatLocalRouteLabel() : provider.name;
  const subtitleEndpoint = internal ? undefined : endpoint;
  return {
    key: `provider:${provider.id}`,
    source: 'provider',
    id: provider.id,
    agentId: provider.agentId,
    title,
    subtitle: providerSubtitle(provider, subtitleEndpoint, mode, t),
    isCurrent: provider.isCurrent,
    auth: {
      status: 'valid',
      health: 'configured',
      label: authHealthLabel('configured', t),
    },
  };
}

function fromTicket(
  ticket: TicketView,
  isCurrent: boolean,
  t?: TranslateFn,
): CredentialRow {
  return {
    key: ticket.id,
    source: ticket.sourceKind,
    id: ticket.sourceId,
    agentId: ticket.agentId,
    title: isInternalGeneratedName(ticket.label) ? formatLocalRouteLabel() : ticket.label,
    subtitle: ticketSubtitle(ticket, t),
    isCurrent,
    auth: {
      status: 'valid',
      health: 'configured',
      label: ticketCredentialClassLabel(ticket.credentialClass, t),
    },
  };
}

/** Single projection used by Connections and ConnectFlow (and ticket wallet). */
export function toCredentialRow(input: CredentialRowInput, t?: TranslateFn): CredentialRow {
  if (input.source === 'account') return fromAccount(input.account, t);
  if (input.source === 'provider') return fromProvider(input.provider, t);
  return fromTicket(input.ticket, input.isCurrent ?? false, t);
}

/** Provider endpoint fields for ConnectionEntry / ticket detail extras. */
export function providerEndpointExtras(provider: Provider): {
  endpoint?: string;
  endpointHost?: string;
  endpointMode: 'official' | 'custom';
} {
  const endpoint = extractProviderEndpoint(provider.configText, provider.configFormat);
  const internal = isInternalGeneratedProvider(provider);
  return {
    endpoint: internal ? undefined : endpoint,
    endpointHost: internal || !endpoint ? undefined : formatEndpointHost(endpoint),
    endpointMode: providerEndpointMode(provider, endpoint),
  };
}

/** API Key account endpoint fields. Missing URL keeps the legacy official mode. */
export function accountEndpointExtras(
  account: Pick<Account, 'kind' | 'agentId' | 'endpoint'>,
): {
  endpoint?: string;
  endpointHost?: string;
  endpointMode?: 'official' | 'custom';
} {
  if (account.kind !== 'apikey') return {};
  const endpoint = account.endpoint?.trim();
  if (!endpoint) return { endpointMode: 'official' };
  const official = looksLikeOfficialEndpoint(account.agentId, endpoint);
  return {
    endpoint,
    endpointHost: formatEndpointHost(endpoint),
    endpointMode: official ? 'official' : 'custom',
  };
}
