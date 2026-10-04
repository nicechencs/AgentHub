/** Marketplace catalogs are vendor concepts exposed by Claude, Codex, and Grok. */
export function canRefreshPluginMarketplace(agent: string): boolean {
  return agent === 'claude' || agent === 'codex' || agent === 'grok';
}

/** Individual package updates are supported only by Claude and Grok. */
export function canUpdateListedPlugin(agent: string, scope?: string | null): boolean {
  return (agent === 'claude' || agent === 'grok') && scope === 'user';
}

/** Pi updates installed extensions as one official CLI operation. */
export function canUpdateAllPlugins(agent: string): boolean {
  return agent === 'pi';
}
