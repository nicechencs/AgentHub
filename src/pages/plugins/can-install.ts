/** These four Agents can install/uninstall packs through their own official CLI. */
export function canInstallListedPlugin(agent: string): boolean {
  return agent === 'claude' || agent === 'codex' || agent === 'grok' || agent === 'pi';
}

export function canUninstallListedPlugin(agent: string): boolean {
  return canInstallListedPlugin(agent);
}
