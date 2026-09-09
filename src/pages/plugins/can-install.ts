/** Claude and Grok can install/uninstall via official CLI. Other agents must not show a fake button. */
export function canInstallListedPlugin(agent: string): boolean {
  return agent === 'claude' || agent === 'grok';
}

export function canUninstallListedPlugin(agent: string): boolean {
  return canInstallListedPlugin(agent);
}
