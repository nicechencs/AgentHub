/**
 * Routes page inspect-pane target types and helpers.
 */
import type { AdapterProfile } from '@/lib/backend/contracts/adapter';
import type { RouteGraphView } from './route-graph-model';

export type WriteTarget = { profile: AdapterProfile; graph: RouteGraphView };

export type RouteInspect =
  | { kind: 'create' }
  | { kind: 'import' }
  | { kind: 'write'; target: WriteTarget }
  | { kind: 'edit'; profile: AdapterProfile }
  | { kind: 'detail'; profile: AdapterProfile };

export function inspectProfileId(target: RouteInspect | null): string | null {
  if (!target) return null;
  if (target.kind === 'edit' || target.kind === 'detail') return target.profile.id;
  if (target.kind === 'write') return target.target.profile.id;
  return null;
}

export function liveInspectProfile(
  snapshot: AdapterProfile,
  profiles: readonly AdapterProfile[],
): AdapterProfile {
  return profiles.find((profile) => profile.id === snapshot.id) ?? snapshot;
}

export const ROUTES_INSPECT_WIDTH_KEY = 'agenthub.routes.inspectWidth';
