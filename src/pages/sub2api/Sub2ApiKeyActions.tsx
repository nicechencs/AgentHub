import type { ReactNode } from 'react';
import { Ban, CheckCircle, Pencil, Trash2 } from 'lucide-react';
import { useI18n } from '@/components/shared/LanguageProvider';
import type { ConnectApiKeyDraft } from '@/lib/connect-flow/connect-intent';
import type { Sub2ApiGroup, Sub2ApiKey } from '@/lib/sub2api';
import type { AgentKey } from '@/lib/types';
import { cn } from '@/lib/utils';
import type { TokenImportAgentRef } from '@/pages/routes/tokens/token-import-model';
import { Sub2ApiImportToAgentButton } from './Sub2ApiImportToAgentButton';
import { nextSub2ApiKeyToggleStatus } from './sub2api-page-model';

function ActionIconButton({
  label,
  disabled,
  onClick,
  tone = 'default',
  testId,
  children,
}: {
  label: string;
  disabled?: boolean;
  onClick: () => void;
  tone?: 'default' | 'danger' | 'enable';
  testId: string;
  children: ReactNode;
}) {
  return (
    <button
      type="button"
      data-sub2api-key-action={testId}
      disabled={disabled}
      aria-label={label}
      onClick={onClick}
      className={cn(
        'inline-flex flex-col items-center gap-0.5 rounded-btn px-1.5 py-1 text-secondary transition-colors',
        'hover:bg-hover disabled:pointer-events-none disabled:opacity-50',
        tone === 'danger' && 'hover:text-danger',
        tone === 'enable' && 'hover:text-success',
        tone === 'default' && 'hover:text-primary',
      )}
    >
      {children}
      <span className="text-xs leading-none">{label}</span>
    </button>
  );
}

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
  const enabling = nextSub2ApiKeyToggleStatus(keyRow.status) === 'active';
  const toggleLabel = enabling ? t('routes.sub2api.enableKey') : t('routes.sub2api.disableKey');

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
      <ActionIconButton
        label={toggleLabel}
        disabled={busy}
        tone={enabling ? 'enable' : 'default'}
        testId={enabling ? 'enable' : 'disable'}
        onClick={() => onToggleStatus(keyRow)}
      >
        {enabling ? <CheckCircle className="h-3.5 w-3.5" /> : <Ban className="h-3.5 w-3.5" />}
      </ActionIconButton>
      <ActionIconButton
        label={t('routes.sub2api.editKey')}
        disabled={busy}
        testId="edit"
        onClick={() => onEdit(keyRow)}
      >
        <Pencil className="h-3.5 w-3.5" />
      </ActionIconButton>
      <ActionIconButton
        label={t('common.delete')}
        disabled={busy}
        tone="danger"
        testId="delete"
        onClick={() => onDelete(keyRow)}
      >
        <Trash2 className="h-3.5 w-3.5" />
      </ActionIconButton>
    </div>
  );
}
