/**
 * Mirror of core `install_lifecycle` (spawn copy source / update / uninstall).
 * UI must not invent a second WorkBuddy special-case outside this table.
 */
export type InstallLifecycleKind = {
  source: 'npm' | 'native' | 'ide' | 'desktop' | 'leftover-agenthub';
  updateVia: 'in_app' | 'ide' | 'desktop' | 'official' | 'none';
  uninstallVia: 'in_app' | 'ide' | 'desktop' | 'official' | 'leftover' | 'none';
};

export function installLifecycle(
  kind: string,
  agentId?: string,
): InstallLifecycleKind {
  if (kind === 'npm') {
    return { source: 'npm', updateVia: 'in_app', uninstallVia: 'in_app' };
  }
  if (kind === 'native' && (agentId === 'workbuddy' || agentId === 'zcode')) {
    return { source: 'native', updateVia: 'official', uninstallVia: 'in_app' };
  }
  if (kind === 'native') {
    return { source: 'native', updateVia: 'in_app', uninstallVia: 'in_app' };
  }
  if (kind === 'ide') {
    return { source: 'ide', updateVia: 'ide', uninstallVia: 'ide' };
  }
  if (kind === 'desktop') {
    return { source: 'desktop', updateVia: 'desktop', uninstallVia: 'desktop' };
  }
  if (kind === 'leftover-agenthub') {
    return { source: 'leftover-agenthub', updateVia: 'none', uninstallVia: 'leftover' };
  }
  return { source: 'native', updateVia: 'none', uninstallVia: 'none' };
}
