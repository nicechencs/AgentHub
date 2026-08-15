import { useCallback, useEffect, useRef, useState } from 'react';
import { analyzeAdapter } from '@/lib/api/adapter';
import type { AdapterRouteAnalysis, AdapterSourceKind } from '@/lib/backend/contracts/adapter';
import type { AgentId } from '@/lib/types';
import { adapterTargetCacheKey, type AdapterTargetAnalysisState } from './adapter-view-model';

export type AdapterTargetAnalyses = Partial<Record<AgentId, AdapterTargetAnalysisState>>;

/**
 * Fan-out read-only `analyze` over the configurable targets of one source.
 * Results are cached per (sourceKind, sourceId, target) for the session;
 * a generation counter drops answers that arrive after the selection moved on.
 * Fan-out is fully disabled (`enabled=false`) e.g. while source OAuth is incomplete.
 */
export function useAdapterTargetAnalyses(input: {
  sourceKind: AdapterSourceKind | null;
  sourceId: string | null;
  targetAgentIds: readonly AgentId[];
  enabled: boolean;
  analyze?: typeof analyzeAdapter;
}): { analyses: AdapterTargetAnalyses; retry: (agentId?: AgentId) => void } {
  const { sourceKind, sourceId, targetAgentIds, enabled } = input;
  const analyze = input.analyze ?? analyzeAdapter;
  const [analyses, setAnalyses] = useState<AdapterTargetAnalyses>({});
  const cache = useRef(new Map<string, AdapterRouteAnalysis>());
  const generation = useRef(0);
  const [retryToken, setRetryToken] = useState(0);

  useEffect(() => {
    const currentGeneration = ++generation.current;
    if (!sourceKind || !sourceId || !enabled) {
      setAnalyses({});
      return;
    }
    const next: AdapterTargetAnalyses = {};
    const pending: AgentId[] = [];
    for (const targetAgentId of targetAgentIds) {
      const cached = cache.current.get(adapterTargetCacheKey({ sourceKind, sourceId, targetAgentId }));
      if (cached) {
        next[targetAgentId] = { kind: 'ready', analysis: cached };
      } else {
        next[targetAgentId] = { kind: 'loading' };
        pending.push(targetAgentId);
      }
    }
    setAnalyses(next);
    for (const targetAgentId of pending) {
      void analyze({ sourceKind, sourceId, targetAgentId })
        .then((analysis) => {
          cache.current.set(adapterTargetCacheKey({ sourceKind, sourceId, targetAgentId }), analysis);
          if (generation.current !== currentGeneration) return;
          setAnalyses((current) => ({ ...current, [targetAgentId]: { kind: 'ready', analysis } }));
        })
        .catch((error: unknown) => {
          if (generation.current !== currentGeneration) return;
          setAnalyses((current) => ({ ...current, [targetAgentId]: { kind: 'error', error } }));
        });
    }
  }, [analyze, enabled, retryToken, sourceId, sourceKind, targetAgentIds]);

  const retry = useCallback((agentId?: AgentId) => {
    if (sourceKind && sourceId) {
      if (agentId) {
        cache.current.delete(adapterTargetCacheKey({ sourceKind, sourceId, targetAgentId: agentId }));
      } else {
        cache.current.clear();
      }
    }
    setRetryToken((token) => token + 1);
  }, [sourceId, sourceKind]);

  return { analyses, retry };
}
