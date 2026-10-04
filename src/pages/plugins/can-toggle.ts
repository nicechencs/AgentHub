/** Pi loads installed packs directly; only Claude, Codex, and Grok expose a real toggle. */
export function canToggleListedPlugin(agent: string): boolean {
  return agent === 'claude' || agent === 'codex' || agent === 'grok';
}
