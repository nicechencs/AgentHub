import { useCallback, useEffect, useMemo, useState } from 'react';
import { useI18n } from '@/components/shared/LanguageProvider';
import { useToast } from '@/components/ui/toast';
import { getChatModel, setChatModel } from '@/lib/api/chat';
import type { TicketView } from '@/lib/backend/contracts/ticket';
import { isRetiredChatModel } from '@/pages/chat/chat-format';
import { piDefaultModelView, type PiDefaultModelView } from './ticket-card-detail';

export function usePiDefaultModel(input: {
  ticket: TicketView | null;
  isCurrent: boolean;
}): {
  view: PiDefaultModelView;
  reload: () => Promise<void>;
  switchModel: (model: string) => Promise<void>;
} {
  const { t } = useI18n();
  const { toast } = useToast();
  const [model, setModel] = useState<string | null>(null);
  const [models, setModels] = useState<string[]>([]);
  const [switching, setSwitching] = useState(false);
  const agentId = input.ticket?.agentId ?? null;

  const reload = useCallback(async () => {
    if (agentId !== 'pi') {
      setModel(null);
      setModels([]);
      return;
    }
    try {
      const live = await getChatModel('pi');
      setModel(live.model && !isRetiredChatModel(live.model) ? live.model : null);
      setModels(live.models.filter((id) => !isRetiredChatModel(id)));
    } catch {
      setModel(null);
      setModels([]);
    }
  }, [agentId]);

  useEffect(() => {
    void reload();
  }, [reload, input.ticket?.id, input.isCurrent]);

  const switchModel = useCallback(async (next: string) => {
    const id = next.trim();
    if (!id || isRetiredChatModel(id) || id === model || switching) return;
    setSwitching(true);
    try {
      await setChatModel('pi', id);
      setModel(id);
      toast({ title: t('connections.list.defaultModelSwitched'), variant: 'success' });
      await reload();
    } catch (error) {
      toast({
        title: t('connections.list.defaultModelSwitchFail'),
        description: error instanceof Error ? error.message : String(error),
        variant: 'danger',
      });
    } finally {
      setSwitching(false);
    }
  }, [model, reload, switching, t, toast]);

  const view = useMemo(
    () => piDefaultModelView({
      agentId,
      isCurrent: input.isCurrent,
      model,
      models,
      switching,
    }),
    [agentId, input.isCurrent, model, models, switching],
  );

  return { view, reload, switchModel };
}
