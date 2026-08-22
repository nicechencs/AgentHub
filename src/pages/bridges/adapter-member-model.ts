/**
 * Routes detail: same-surface members + picker health.
 * Member list reuses C1 surfaceGroups; health prefers the optional wire
 * field, then ConnectionEntry AuthHealth overlay.
 */
import type { AdapterProfile } from '@/lib/backend/contracts/adapter';
import {
  isIsolatedMemberHealth,
  memberHealthFromAuthHealth,
  surfaceGroupForTicketId,
  ticketIdFor,
  ticketMemberHealthLabel,
  type TicketMemberHealth,
  type TicketSurfaceGroupView,
  type TicketSurfaceMemberView,
} from '@/lib/backend/contracts/ticket';
import type { ConnectionEntry } from '@/lib/connection-entry';
import type { AgentId } from '@/lib/types';
import type { TranslateFn } from '@/lib/i18n';

export type BridgeMemberRow = {
  ticketId: string;
  sourceKind: TicketSurfaceMemberView['sourceKind'];
  sourceId: string;
  agentId: AgentId | null;
  label: string;
  health: TicketMemberHealth;
  reason: string;
  isolated: boolean;
  lead: boolean;
  missing: boolean;
};

function memberHealthLabel(health: TicketMemberHealth, t?: TranslateFn): string {
  if (!t) return ticketMemberHealthLabel(health);
  if (health === 'needs_login') return t('routes.members.needsLogin');
  if (health === 'try_once') return t('routes.members.tryOnce');
  return t('routes.members.renewable');
}

function missingReason(t?: TranslateFn): string {
  return t ? t('routes.members.missing') : '来源连接已删除';
}

function entryForMember(
  member: Pick<TicketSurfaceMemberView, 'sourceKind' | 'sourceId'>,
  entries: readonly ConnectionEntry[],
): ConnectionEntry | undefined {
  return entries.find(
    (entry) => entry.source === member.sourceKind && entry.id === member.sourceId,
  );
}

function overlayHealth(
  member: TicketSurfaceMemberView,
  entry: ConnectionEntry | undefined,
): TicketMemberHealth {
  if (member.health) return member.health;
  return memberHealthFromAuthHealth(entry?.authHealth);
}

function identityLabel(
  member: TicketSurfaceMemberView,
  entry: ConnectionEntry | undefined,
): string {
  const identity = entry?.identityLabel?.trim();
  if (identity) return identity;
  const title = entry?.title?.trim();
  if (title) return title;
  return member.label;
}

export function surfaceGroupForProfile(
  groups: readonly TicketSurfaceGroupView[],
  profile: Pick<AdapterProfile, 'sourceKind' | 'sourceId'>,
): TicketSurfaceGroupView | undefined {
  return surfaceGroupForTicketId(groups, ticketIdFor(profile.sourceKind, profile.sourceId));
}

export function bridgeMemberRows(input: {
  profile: Pick<AdapterProfile, 'sourceKind' | 'sourceId' | 'name'>;
  groups: readonly TicketSurfaceGroupView[];
  entries: readonly ConnectionEntry[];
  t?: TranslateFn;
}): BridgeMemberRow[] {
  const { profile, groups, entries, t } = input;
  const leadId = ticketIdFor(profile.sourceKind, profile.sourceId);
  const group = surfaceGroupForProfile(groups, profile);
  const members = group?.members ?? [];

  const seen = new Set<string>();
  const rows: BridgeMemberRow[] = [];
  for (const member of members) {
    if (seen.has(member.ticketId)) continue;
    seen.add(member.ticketId);
    const entry = entryForMember(member, entries);
    const missing = !entry;
    const health = missing ? 'needs_login' : overlayHealth(member, entry);
    const isolated = missing || isIsolatedMemberHealth(health);
    rows.push({
      ticketId: member.ticketId,
      sourceKind: member.sourceKind,
      sourceId: member.sourceId,
      agentId: entry?.agentId ?? member.agentId,
      label: identityLabel(member, entry),
      health,
      reason: missing ? missingReason(t) : memberHealthLabel(health, t),
      isolated,
      lead: member.ticketId === leadId,
      missing,
    });
  }

  if (!seen.has(leadId)) {
    const entry = entries.find(
      (item) => item.source === profile.sourceKind && item.id === profile.sourceId,
    );
    const missing = !entry;
    rows.unshift({
      ticketId: leadId,
      sourceKind: profile.sourceKind,
      sourceId: profile.sourceId,
      agentId: entry?.agentId ?? null,
      label: entry?.identityLabel?.trim() || entry?.title || profile.name,
      health: 'needs_login',
      reason: missingReason(t),
      isolated: true,
      lead: true,
      missing,
    });
  }

  return rows;
}

export function memberPinTone(
  row: Pick<BridgeMemberRow, 'isolated' | 'missing' | 'health'>,
): 'success' | 'warning' | 'danger' | 'muted' {
  if (row.missing) return 'muted';
  if (row.health === 'needs_login' || row.isolated) return 'danger';
  if (row.health === 'try_once') return 'warning';
  return 'success';
}
