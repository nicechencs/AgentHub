import { useEffect, useRef, useState } from 'react';
import { probeLiveAuth, type LiveAuthProbe } from '@/lib/api/account';
import type { AgentKey } from '@/lib/types';

/**
 * 导入对话框的本机登录探测：打开时 `probeLiveAuth({ force: true })`，
 * 用 generation 丢弃过期结果。不含切换 / 绑定 / 删除 / 分享 / 路由侧栏。
 */
export function useConnectionImportProbe(input: {
  addAgentId: AgentKey;
  discoveryProbe: LiveAuthProbe | null;
}) {
  const { addAgentId, discoveryProbe } = input;
  const [loginImportOpen, setLoginImportOpen] = useState(false);
  const [importLiveProbe, setImportLiveProbe] = useState<LiveAuthProbe | null>(null);
  const [importProbeLoading, setImportProbeLoading] = useState(false);
  const importProbeGen = useRef(0);
  const [importingAccount, setImportingAccount] = useState(false);

  useEffect(() => {
    if (!loginImportOpen) {
      importProbeGen.current += 1;
      setImportLiveProbe(null);
      setImportProbeLoading(false);
      return;
    }
    const generation = ++importProbeGen.current;
    const agentId = addAgentId;
    const seed = discoveryProbe?.agentId === agentId ? discoveryProbe : null;
    setImportLiveProbe(seed);
    setImportProbeLoading(!seed);
    void probeLiveAuth(agentId, { force: true }).then(
      (probe) => {
        if (importProbeGen.current !== generation) return;
        setImportLiveProbe(probe);
        setImportProbeLoading(false);
      },
      () => {
        if (importProbeGen.current !== generation) return;
        setImportLiveProbe(null);
        setImportProbeLoading(false);
      },
    );
    // eslint-disable-next-line react-hooks/exhaustive-deps -- seed at open; listing discoveryProbe would re-force
  }, [addAgentId, loginImportOpen]);

  return {
    loginImportOpen,
    setLoginImportOpen,
    importLiveProbe,
    setImportLiveProbe,
    importProbeLoading,
    importingAccount,
    setImportingAccount,
  };
}
