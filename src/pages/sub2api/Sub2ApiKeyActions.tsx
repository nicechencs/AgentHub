import { Pencil, Trash2 } from 'lucide-react';
import { useI18n } from '@/components/shared/LanguageProvider';
import { Button } from '@/components/ui/button';
import { Switch } from '@/components/ui/switch';
import type { ConnectApiKeyDraft } from '@/lib/connect-flow/connect-intent';
import type { Sub2ApiGroup, Sub2ApiKey } from '@/lib/sub2api';
import type { AgentKey } from '@/lib/types';
import type { TokenImportAgentRef } from '@/pages/routes/tokens/token-import-model';
import { Sub2ApiImportToAgentButton } from './Sub2ApiImportToAgentButton';
import { sub2apiKeyStatusKind } from './sub2api-page-model';

export function Sub2ApiKeyActions({
  keyRow,
  groups,
  gatewayBaseUrl,
  installedAgents,
  onImport,
  onToggleStatus,
  onEdit,
  onDelete,
  busy = false,
}: {
  keyRow: Sub2ApiKey;
  groups: readonly Sub2ApiGroup[];
  gatewayBaseUrl: string;
  installedAgents: readonly TokenImportAgentRef[];
  onImport: (agentId: AgentKey, draft: ConnectApiKeyDraft) => void;
  onToggleStatus: (key: Sub2ApiKey) => void;
  onEdit: (key: Sub2ApiKey) => void;
  onDelete: (key: Sub2ApiKey) => void;
  busy?: boolean;
}) {
  const { t } = useI18n();
  const active = sub2apiKeyStatusKind(keyRow.status) === 'active';
  const toggleLabel = active ? t('routes.sub2api.disableKey') : t('routes.sub2api.enableKey');

  return (
    <div className="flex items-center gap-0.5" data-sub2api-key-actions="">
      <Sub2ApiImportToAgentButton
        keyRow={keyRow}
        groups={groups}
        gatewayBaseUrl={gatewayBaseUrl}
        installedAgents={installedAgents}
        onImport={onImport}
        busy={busy}
      />
      <Switch
        checked={active}
        disabled={busy}
        aria-label={toggleLabel}
        data-sub2api-key-action={active ? 'disable' : 'enable'}
        onCheckedChange={() => onToggleStatus(keyRow)}
      />
      <Button
        type="button"
        size="icon"
        variant="ghost"
        data-sub2api-key-action="edit"
        disabled={busy}
        title={t('routes.sub2api.editKey')}
        aria-label={t('routes.sub2api.editKey')}
        onClick={() => onEdit(keyRow)}
      >
        <Pencil className="h-3.5 w-3.5" />
      </Button>
      <Button
        type="button"
        size="icon"
        variant="ghost"
        className="text-danger hover:text-danger"
        data-sub2api-key-action="delete"
        disabled={busy}
        title={t('common.delete')}
        aria-label={t('common.delete')}
        onClick={() => onDelete(keyRow)}
      >
        <Trash2 className="h-3.5 w-3.5" />
      </Button>
    </div>
  );
}
