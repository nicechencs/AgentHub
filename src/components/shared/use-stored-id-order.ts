import { useCallback, useEffect, useState } from 'react';
import { mergeLiveMove, persistIdOrder, readIdOrder, subscribeIdOrder } from '@/lib/list-order';

export function useStoredIdOrder(storageKey: string) {
  const [stored, setStored] = useState<string[]>(() => readIdOrder(storageKey));

  useEffect(() => {
    setStored(readIdOrder(storageKey));
    return subscribeIdOrder(storageKey, () => {
      queueMicrotask(() => {
        setStored(readIdOrder(storageKey));
      });
    });
  }, [storageKey]);

  const seedIfEmpty = useCallback(
    (liveIds: readonly string[]) => {
      if (liveIds.length === 0) return;
      setStored((current) => {
        if (current.length > 0) return current;
        const next = [...liveIds];
        persistIdOrder(storageKey, next);
        return next;
      });
    },
    [storageKey],
  );

  const moveInLive = useCallback(
    (liveIds: readonly string[], fromId: string, toId: string) => {
      if (fromId === toId) return;
      setStored((current) => {
        const next = mergeLiveMove(current, liveIds, fromId, toId);
        persistIdOrder(storageKey, next);
        return next;
      });
    },
    [storageKey],
  );

  return { stored, seedIfEmpty, moveInLive };
}
