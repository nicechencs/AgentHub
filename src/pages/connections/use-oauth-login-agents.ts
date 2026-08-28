/** Which installed Agents offer in-app official login. */
import { useEffect, useState } from 'react';
import { oauthSupported } from '@/lib/api/account';
import type { AgentId } from '@/lib/types';

const EMPTY: readonly AgentId[] = [];

export function useOAuthLoginAgents(
  agentIds?: readonly AgentId[] | null,
): readonly AgentId[] {
  const key = (agentIds ?? EMPTY).join('\0');
  const [supported, setSupported] = useState<readonly AgentId[]>(EMPTY);

  useEffect(() => {
    let cancelled = false;
    const current = key ? key.split('\0') : [];
    if (current.length === 0) {
      setSupported(EMPTY);
      return;
    }
    setSupported((prev) => prev.filter((id) => current.includes(id)));
    void Promise.all(
      current.map((id) =>
        oauthSupported(id)
          .then((ok) => [id, ok] as const)
          .catch(() => [id, false] as const),
      ),
    ).then((rows) => {
      if (!cancelled) {
        setSupported(rows.filter(([, ok]) => ok).map(([id]) => id));
      }
    });
    return () => {
      cancelled = true;
    };
  }, [key]);

  return supported;
}
