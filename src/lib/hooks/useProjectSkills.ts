import { useCallback, useEffect, useState } from 'react';
import {
  listProjectSkills,
  type InstalledSkillDto,
} from '@/lib/api/skill';

export function useProjectSkills(workspacePath: string | null, enabled: boolean) {
  const [data, setData] = useState<InstalledSkillDto[] | null>(null);
  const [error, setError] = useState<unknown>(null);
  const [fetching, setFetching] = useState(false);

  const reload = useCallback(async () => {
    if (!enabled || !workspacePath) return;
    setFetching(true);
    try {
      const rows = await listProjectSkills(workspacePath);
      setData(rows);
      setError(null);
    } catch (err) {
      setError(err);
    } finally {
      setFetching(false);
    }
  }, [enabled, workspacePath]);

  useEffect(() => {
    if (!enabled || !workspacePath) {
      setData(null);
      setError(null);
      return;
    }
    let cancelled = false;
    setData(null);
    setError(null);
    setFetching(true);
    void listProjectSkills(workspacePath)
      .then((rows) => {
        if (cancelled) return;
        setData(rows);
        setError(null);
      })
      .catch((err) => {
        if (cancelled) return;
        setError(err);
      })
      .finally(() => {
        if (!cancelled) setFetching(false);
      });
    return () => {
      cancelled = true;
    };
  }, [enabled, workspacePath]);

  return {
    data,
    error,
    loading: enabled && Boolean(workspacePath) && data == null && error == null,
    refreshing: enabled && fetching && data != null,
    reload,
  };
}
