/** Must match core `canonical_usage_model`. */
export function canonicalUsageModel(raw: string): string {
  let s = raw.trim();
  if (s.startsWith('[grok] ')) s = s.slice('[grok] '.length).trim();
  if (s.startsWith('xai/')) s = s.slice('xai/'.length);
  else if (s.startsWith('x-ai/')) s = s.slice('x-ai/'.length);
  s = s.trim();
  if (s.endsWith('-build')) {
    const base = s.slice(0, -'-build'.length).trim();
    if (base === 'grok' || base.startsWith('grok-')) return base;
  }
  return s;
}

export function usageModelsMatch(rowModel: string, selected: string): boolean {
  if (!selected || selected === 'all') return true;
  return canonicalUsageModel(rowModel) === canonicalUsageModel(selected);
}
