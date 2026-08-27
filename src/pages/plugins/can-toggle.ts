/** Claude and Grok listed packs can be turned on/off. Planned/unsupported agents must not show fake buttons. */
export function canToggleListedPlugin(agent: string): boolean {
  return agent === 'claude' || agent === 'grok';
}
