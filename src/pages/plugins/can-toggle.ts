/** Claude and Grok listed packs can be turned on/off. Planned/unsupported agents must not show a fake toggle. */
export function canToggleListedPlugin(agent: string): boolean {
  return agent === 'claude' || agent === 'grok';
}
