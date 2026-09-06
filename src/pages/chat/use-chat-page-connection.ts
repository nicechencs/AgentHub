import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { useTicketWallet } from '@/app/runtime';
import { useI18n } from '@/components/shared/LanguageProvider';
import { useToast } from '@/components/ui/toast';
import { switchAccount } from '@/lib/api/account';
import { getChatModel, setChatEffort, setChatModel } from '@/lib/api/chat';
import {
  listProviders,
  listRemoteOpenAiModelsForProvider,
  switchProvider,
  upsertProvider,
} from '@/lib/api/provider';
import { bindTicket, isActiveBindingForAgent } from '@/lib/api/tickets';
import {
  describeProviderSwitchError,
  switchWroteLiveLabel,
} from '@/pages/connections/use-connection-page-actions';
import type { TicketWallet } from '@/lib/backend/contracts/ticket';
import type { AgentKey, AgentStatus, Conversation, Provider } from '@/lib/types';
import { applyFormVars, extractFormVars } from '@/lib/provider-detect';
import { filterRemoteModelsForAgent } from '@/lib/provider-detect/remote-models';
import {
  chatModelOptions,
  extractModel,
  extractPiDefaultProvider,
  extractPiSlotModels,
  isRetiredChatModel,
  officialPiModelsBaseUrl,
  piChatModelOptions,
  resolvePiChatCurrentModel,
  shouldFetchChatRemoteModels,
} from './chat-format';
import {
  chatConnectionOptions,
  chatConnectionPickerView,
  connectionPickerCaption,
  leftoverProviderIsCurrent,
} from './chat-model';

/**
 * Chat 连接切换与票夹订阅。
 * switch-account → switch-provider → bind 顺序不变；refreshAgents 由会话 Hook 传入。
 * 不含发送、取消、切会话、世代。
 */
const EMPTY_WALLET: TicketWallet = { tickets: [], bindings: [], surfaceGroups: [] };

export function useChatPageConnection(input: {
  primaryAgent: AgentKey | null;
  active: Conversation | null;
  hiddenIds: Set<AgentKey>;
  agentStatus: AgentStatus[];
  refreshAgents: (opts?: { force?: boolean }) => Promise<AgentStatus[]>;
}) {
  const { t } = useI18n();
  const { toast } = useToast();
  const { primaryAgent, active, hiddenIds, agentStatus, refreshAgents } = input;
  const ticketWallet = useTicketWallet();
  const [providers, setProviders] = useState<Provider[]>([]);
  const [remoteModels, setRemoteModels] = useState<string[]>([]);
  const [liveChatModel, setLiveChatModel] = useState<string | null>(null);
  const [liveChatModels, setLiveChatModels] = useState<string[]>([]);
  const [liveChatEffort, setLiveChatEffort] = useState<string | null>(null);
  const [liveChatEfforts, setLiveChatEfforts] = useState<string[]>([]);
  const [switchingModel, setSwitchingModel] = useState(false);
  const [switchingEffort, setSwitchingEffort] = useState(false);
  const wallet = ticketWallet.wallet;
  const walletReady = ticketWallet.state === 'ready' || ticketWallet.state === 'error';
  const providersGenRef = useRef(0);
  const [switchingProvider, setSwitchingProvider] = useState(false);

  const currentProvider = useMemo(
    () => providers.find((p) => p.isCurrent) ?? null,
    [providers],
  );

  const loadProviders = useCallback(async (agentId: AgentKey) => {
    const gen = ++providersGenRef.current;
    try {
      const list = await listProviders(agentId);
      if (gen !== providersGenRef.current) return;
      setProviders(list);
    } catch {
      if (gen !== providersGenRef.current) return;
      setProviders([]);
    }
  }, []);

  useEffect(() => {
    void ticketWallet.ensureLoaded();
  }, [ticketWallet.ensureLoaded]);

  useEffect(() => {
    if (!primaryAgent) {
      providersGenRef.current += 1;
      setProviders([]);
      return;
    }
    setProviders([]);
    void loadProviders(primaryAgent);
  }, [primaryAgent, loadProviders]);

  const primaryStatus = useMemo(
    () => (primaryAgent ? agentStatus.find((a) => a.agentId === primaryAgent) : undefined),
    [agentStatus, primaryAgent],
  );

  const leftoverCurrent = leftoverProviderIsCurrent(providers);

  const usesLiveChatModel = primaryAgent === 'pi' || primaryAgent === 'grok';

  const loadLiveChatModel = useCallback(async (agentId: AgentKey) => {
    if (agentId !== 'pi' && agentId !== 'grok') {
      setLiveChatModel(null);
      setLiveChatModels([]);
      setLiveChatEffort(null);
      setLiveChatEfforts([]);
      return;
    }
    try {
      const live = await getChatModel(agentId);
      setLiveChatModel(live.model && !isRetiredChatModel(live.model) ? live.model : null);
      setLiveChatModels(live.models.filter((id) => !isRetiredChatModel(id)));
      setLiveChatEffort(live.effort?.trim() || null);
      setLiveChatEfforts((live.efforts ?? []).map((id) => id.trim()).filter(Boolean));
    } catch {
      setLiveChatModel(null);
      setLiveChatModels([]);
      setLiveChatEffort(null);
      setLiveChatEfforts([]);
    }
  }, []);

  useEffect(() => {
    if (!usesLiveChatModel || !primaryAgent) {
      setLiveChatModel(null);
      setLiveChatModels([]);
      setLiveChatEffort(null);
      setLiveChatEfforts([]);
      return;
    }
    void loadLiveChatModel(primaryAgent);
  }, [primaryAgent, currentProvider, loadLiveChatModel, usesLiveChatModel]);

  useEffect(() => {
    if (!currentProvider?.id) {
      setRemoteModels([]);
      return;
    }
    let cancelled = false;
    const vars = extractFormVars(currentProvider.agentId, currentProvider.configText, currentProvider.configFormat);
    const baseUrl =
      vars.baseUrl?.trim()
      || (primaryAgent === 'pi'
        ? officialPiModelsBaseUrl(extractPiDefaultProvider(currentProvider.configText))
        : '');
    if (!shouldFetchChatRemoteModels(currentProvider.id, baseUrl)) {
      setRemoteModels([]);
      return;
    }
    void listRemoteOpenAiModelsForProvider(currentProvider.id, baseUrl)
      .then((ids) => {
        if (cancelled) return;
        const agentFilter = primaryAgent === 'pi' && /api\.x\.ai/i.test(baseUrl)
          ? 'grok'
          : currentProvider.agentId;
        setRemoteModels(filterRemoteModelsForAgent(agentFilter, ids));
      })
      .catch(() => {
        if (!cancelled) setRemoteModels([]);
      });
    return () => {
      cancelled = true;
    };
  }, [currentProvider, primaryAgent]);

  const currentModel = useMemo(() => {
    if (leftoverCurrent) return null;
    if (primaryAgent === 'pi' || primaryAgent === 'grok') {
      return resolvePiChatCurrentModel(liveChatModel);
    }
    const fromProvider = currentProvider ? extractModel(currentProvider.configText) : null;
    if (fromProvider && !isRetiredChatModel(fromProvider)) return fromProvider;
    return null;
  }, [currentProvider, leftoverCurrent, liveChatModel, primaryAgent]);

  const modelOptions = useMemo(() => {
    if (primaryAgent === 'pi') {
      return piChatModelOptions({
        remoteModels,
        liveModels: liveChatModels,
        envelopeModels: currentProvider ? extractPiSlotModels(currentProvider.configText) : [],
        currentModel,
      });
    }
    if (primaryAgent === 'grok') {
      return chatModelOptions(liveChatModels, currentModel);
    }
    return chatModelOptions(remoteModels, currentModel);
  }, [currentModel, currentProvider, liveChatModels, primaryAgent, remoteModels]);

  const effortOptions = usesLiveChatModel ? liveChatEfforts : [];
  const currentEffort = usesLiveChatModel ? liveChatEffort : null;

  const connectionOptions = useMemo(
    () =>
      chatConnectionOptions(t, {
        wallet: wallet ?? (walletReady ? EMPTY_WALLET : null),
        agentId: primaryAgent,
      }),
    [wallet, walletReady, primaryAgent, t],
  );

  const activeLogin = connectionOptions.find((option) => option.isCurrent) ?? null;

  const connectionView = useMemo(
    () =>
      chatConnectionPickerView(t, {
        primaryAgent,
        switching: switchingProvider,
        status: primaryStatus,
        currentProviderName: leftoverCurrent ? null : currentProvider?.name ?? null,
        currentProviderModel: leftoverCurrent
          ? null
          : currentProvider
            ? extractModel(currentProvider.configText)
            : null,
        activeLogin: activeLogin
          ? { title: activeLogin.title, subtitle: activeLogin.subtitle }
          : null,
        leftoverCurrent,
        walletReady,
      }),
    [
      primaryAgent,
      switchingProvider,
      primaryStatus,
      leftoverCurrent,
      currentProvider,
      activeLogin,
      walletReady,
      t,
    ],
  );

  const connectionCaption = useMemo(
    () =>
      active
        ? connectionPickerCaption(t, {
            agentIds: active.agentIds,
            primaryAgent,
          })
        : null,
    [active, primaryAgent, t],
  );

  async function handleSwitchConnection(ticketId: string) {
    if (!primaryAgent || switchingProvider || hiddenIds.has(primaryAgent)) return;
    const option = connectionOptions.find((row) => row.ticketId === ticketId);
    if (!option || option.isCurrent) return;
    setSwitchingProvider(true);
    try {
      const wroteLocal =
        option.action.type === 'switch-account' ||
        option.action.type === 'switch-provider';
      if (option.action.type === 'switch-account') {
        await switchAccount(primaryAgent, option.action.accountId);
      } else if (option.action.type === 'switch-provider') {
        await switchProvider(primaryAgent, option.action.providerId);
      } else {
        const { binding } = await bindTicket(option.action.ticketId, primaryAgent);
        if (!isActiveBindingForAgent(binding, primaryAgent)) {
          throw new Error(t('chat.connection.bindNotCurrent'));
        }
      }
      await Promise.all([
        ticketWallet.reload(),
        loadProviders(primaryAgent),
        loadLiveChatModel(primaryAgent),
        refreshAgents({ force: true }).catch(() => []),
      ]);
      toast({
        title: wroteLocal ? switchWroteLiveLabel(t) : t('chat.connection.switched'),
        variant: 'success',
      });
    } catch (e) {
      toast({
        title: t('connections.list.switchFail'),
        description: describeProviderSwitchError(primaryAgent, e, t),
        variant: 'danger',
      });
    } finally {
      setSwitchingProvider(false);
    }
  }

  async function handleSwitchModel(model: string) {
    if (!primaryAgent || switchingProvider || switchingModel || hiddenIds.has(primaryAgent)) return;
    const next = model.trim();
    if (!next || isRetiredChatModel(next) || next === currentModel) return;
    setSwitchingModel(true);
    try {
      if (primaryAgent === 'pi' || primaryAgent === 'grok') {
        await setChatModel(primaryAgent, next);
        setLiveChatModel(next);
      } else if (currentProvider && currentProvider.agentId === primaryAgent) {
        const vars = extractFormVars(
          currentProvider.agentId,
          currentProvider.configText,
          currentProvider.configFormat,
        );
        const configText = applyFormVars(
          currentProvider.agentId,
          currentProvider.configText,
          currentProvider.configFormat,
          { ...vars, model: next },
        );
        const saved = await upsertProvider({ ...currentProvider, configText });
        await switchProvider(primaryAgent, saved.id);
      } else {
        throw new Error(t('chat.composer.modelUnavailable'));
      }
      await Promise.all([loadProviders(primaryAgent), loadLiveChatModel(primaryAgent)]);
      toast({
        title: t('chat.composer.modelSwitched'),
        variant: 'success',
      });
    } catch (e) {
      toast({
        title: t('chat.composer.modelSwitchFail'),
        description: e instanceof Error ? e.message : String(e),
        variant: 'danger',
      });
    } finally {
      setSwitchingModel(false);
    }
  }

  async function handleSwitchEffort(effort: string) {
    if (!primaryAgent || !usesLiveChatModel || switchingProvider || switchingModel || switchingEffort || hiddenIds.has(primaryAgent)) return;
    const next = effort.trim();
    if (!next || next === currentEffort) return;
    setSwitchingEffort(true);
    try {
      await setChatEffort(primaryAgent, next);
      setLiveChatEffort(next);
      await loadLiveChatModel(primaryAgent);
      toast({
        title: t('chat.composer.modelSwitched'),
        variant: 'success',
      });
    } catch (e) {
      toast({
        title: t('chat.composer.modelSwitchFail'),
        description: e instanceof Error ? e.message : String(e),
        variant: 'danger',
      });
    } finally {
      setSwitchingEffort(false);
    }
  }

  return {
    providers,
    switchingProvider,
    switchingModel: switchingModel || switchingEffort,
    modelOptions,
    currentModel,
    effortOptions,
    currentEffort,
    handleSwitchEffort,
    connectionView,
    connectionOptions,
    connectionCaption,
    walletError: ticketWallet.error,
    reloadWallet: ticketWallet.reload,
    handleSwitchConnection,
    handleSwitchModel,
  };
}
