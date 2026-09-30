import {
  authHealthLabel,
  type AuthHealth,
} from '@/lib/backend/contracts/auth-state';
import { connectionKindLabel } from '@/lib/connection-kind';
import type { TranslateFn } from '@/lib/i18n';
import type {
  LoginIdentityInput,
  LoginPresentation,
  LoginStatusInput,
} from './types';

const PLACEHOLDER_IDENTITY = new Set([
  '官方未提供账号信息',
  '官方未提供登录信息',
  'codex-oauth',
  'codex oauth',
  'grok-oauth',
  'kimi-oauth',
  'claude-oauth',
  'pi-auth',
]);

function isPlaceholderIdentity(value: string): boolean {
  const text = value.trim().toLowerCase();
  if (!text || text.startsWith('**')) return true;
  if (PLACEHOLDER_IDENTITY.has(text)) return true;
  return /\(oauth\)$/i.test(text)
    || / · oauth$/i.test(text)
    || / oauth$/i.test(text)
    || /-oauth$/i.test(text);
}

/** Hostname only. Paths, userinfo, and query strings stay off the login label. */
export function displayEndpointHost(raw?: string): string {
  const text = raw?.trim();
  if (!text) return '';
  const withScheme = /^https?:\/\//i.test(text) ? text : `https://${text}`;
  try {
    return new URL(withScheme).hostname;
  } catch {
    return text.split('/')[0]?.split('@').pop()?.split(':')[0] ?? '';
  }
}

export function acceptedIdentityLabel(value?: string): string | undefined {
  const text = value?.trim();
  if (!text || isPlaceholderIdentity(text)) return undefined;
  return text;
}

function apiKeyPrimary(input: LoginIdentityInput, host: string): string {
  const label = input.label.trim();
  if (!label) return host;
  if (
    input.endpointMode === 'custom'
    && host
    && (label.includes('/') || label.includes(host))
  ) {
    const head = label.split(' · ')[0]?.trim() ?? '';
    if (head && head !== label && !head.includes('/') && !head.includes(host)) return head;
    return host;
  }
  return label;
}

export function presentLoginIdentity(
  input: LoginIdentityInput,
  t: TranslateFn,
): LoginPresentation['identity'] {
  if (input.kind === 'apikey') {
    const host = displayEndpointHost(input.endpointHost);
    const primary = apiKeyPrimary(input, host);
    const kindLabel = connectionKindLabel('apikey', t);
    const secondary = host && primary !== host ? `${kindLabel} · ${host}` : kindLabel;
    return secondary ? { primary, secondary } : { primary };
  }
  const identity = acceptedIdentityLabel(input.identityLabel);
  return { primary: identity ?? input.label.trim() };
}

function statusHealth(input: LoginStatusInput): AuthHealth {
  if (input.authHealth) return input.authHealth;
  if (input.authStatus === 'expired') return 'needs_login';
  if (input.authStatus === 'none') return 'missing';
  return 'unknown';
}

function statusTone(health: AuthHealth): LoginPresentation['status']['tone'] {
  if (health === 'needs_login') return 'danger';
  if (health === 'missing' || health === 'unknown') return 'muted';
  if (health === 'configured') return 'warning';
  return 'success';
}

export function presentLoginStatus(
  input: LoginStatusInput,
  t: TranslateFn,
): LoginPresentation['status'] {
  // Legacy AuthStatus.expiring has no AuthHealth twin; keep Connections list semantics.
  if (!input.authHealth && input.authStatus === 'expiring') {
    return {
      label: t('chrome.authStatus.expiring'),
      tone: 'warning',
    };
  }
  if (input.inTrash) {
    return { label: t('kind.health.inTrash'), tone: 'warning' };
  }
  if (input.memberUnhealthy) {
    return { label: t('kind.health.memberUnhealthy'), tone: 'warning' };
  }
  if (input.catalogEmpty) {
    return { label: t('kind.health.catalogEmpty'), tone: 'warning' };
  }
  const health = statusHealth(input);
  return {
    label: authHealthLabel(health, t),
    tone: statusTone(health),
  };
}

/** Identity and status for one login. Ignores every field outside the display input. */
export function presentLogin(
  input: LoginIdentityInput & LoginStatusInput,
  t: TranslateFn,
): LoginPresentation {
  return {
    identity: presentLoginIdentity(input, t),
    status: presentLoginStatus(input, t),
    kind: input.kind,
  };
}
