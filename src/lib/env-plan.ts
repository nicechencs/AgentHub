/**
 * Pure env install planning (no I/O). Shared by tauri + mock env ports.
 */
import { RUNTIME_MAP } from '@/config/runtimes';
import type { RuntimeDetect, RuntimeId } from '@/lib/types';

export interface AutoInstallPlan {
  targets: RuntimeId[];
  skipped: RuntimeId[];
  summary: string;
}

export function resolveAutoInstallPlan(
  runtimes: RuntimeDetect[],
  onlyIds?: RuntimeId[],
): AutoInstallPlan {
  const filter = onlyIds ? new Set(onlyIds) : null;
  const issues = runtimes.filter(
    (r) => r.status !== 'ok' && (!filter || filter.has(r.id)),
  );
  const issueIds = new Set(issues.map((i) => i.id));
  const targets: RuntimeId[] = [];
  const skipped: RuntimeId[] = [];

  if (issueIds.has('nodejs') || issueIds.has('npm')) {
    if (RUNTIME_MAP.nodejs.canAutoInstall) {
      targets.push('nodejs');
    } else {
      if (issueIds.has('nodejs')) skipped.push('nodejs');
      if (issueIds.has('npm')) skipped.push('npm');
    }
  }

  for (const id of issueIds) {
    if (id === 'nodejs' || id === 'npm') continue;
    if (RUNTIME_MAP[id].canAutoInstall) targets.push(id);
    else skipped.push(id);
  }

  const summaryParts = targets.map((id) => {
    if (id === 'nodejs' && (issueIds.has('npm') || issueIds.has('nodejs'))) {
      return 'Node.js(含 npm)';
    }
    return RUNTIME_MAP[id].name;
  });

  return {
    targets,
    skipped,
    summary: summaryParts.length ? summaryParts.join('、') : '无',
  };
}
