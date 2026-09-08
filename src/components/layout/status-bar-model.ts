import type { MessageKey } from '@/lib/i18n';

export type StatusBarForwardKind = 'running' | 'stopped' | 'restarting' | 'unavailable';

export function statusBarForwardKind(input: {
  available: boolean;
  running?: boolean;
  restarting?: boolean;
}): StatusBarForwardKind {
  if (!input.available) return 'unavailable';
  if (input.restarting) return 'restarting';
  return input.running ? 'running' : 'stopped';
}

export function statusBarForwardMessageKey(kind: StatusBarForwardKind): MessageKey {
  switch (kind) {
    case 'running':
      return 'routes.runtime.running';
    case 'restarting':
      return 'routes.localForward.restarting';
    case 'unavailable':
      return 'routes.runtime.unavailable';
    default:
      return 'routes.runtime.stopped';
  }
}

export function statusBarForwardDotClass(kind: StatusBarForwardKind): string {
  switch (kind) {
    case 'running':
      return 'bg-success';
    case 'restarting':
      return 'bg-warning';
    default:
      return 'bg-muted';
  }
}
