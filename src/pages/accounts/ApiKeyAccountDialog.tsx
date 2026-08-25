import * as React from 'react';
import { SideInspectPanel } from '@/components/layout/SideInspectPanel';
import {
  Dialog,
  DialogContent,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select';
import { SecretInput } from '@/components/shared/SecretInput';
import { Hint } from '@/components/ui/tooltip';
import { useI18n } from '@/components/shared/LanguageProvider';
import { useToast } from '@/components/ui/toast';
import { addApiKeyAccount, updateApiKeyAccount } from '@/lib/api/account';
import { resolveAgentMeta } from '@/config/agents';
import type { Account, AgentId } from '@/lib/types';

const CLAUDE_ENV_KEYS = [
  { value: 'ANTHROPIC_AUTH_TOKEN', label: 'ANTHROPIC_AUTH_TOKEN（默认 Bearer）' },
  { value: 'ANTHROPIC_API_KEY', label: 'ANTHROPIC_API_KEY（x-api-key）' },
] as const;

export type ApiKeyDialogMode = 'add' | 'edit';

/**
 * API Key 账号新增 / 编辑配置页。
 * - 新增：名称可选 + Key 必填
 * - 编辑：名称可改；Key 留空则保留原密钥（界面从不回显明文）
 */
export function ApiKeyAccountDialog({
  agentId,
  open,
  onOpenChange,
  mode = 'add',
  account,
  onSaved,
  asPanel = false,
  width,
}: {
  agentId: AgentId;
  open: boolean;
  onOpenChange: (v: boolean) => void;
  mode?: ApiKeyDialogMode;
  /** 编辑模式目标账号 */
  account?: Account | null;
  onSaved: (acc: Account) => void;
  asPanel?: boolean;
  width?: number;
}) {
  const { t } = useI18n();
  const { toast } = useToast();
  const [label, setLabel] = React.useState('');
  const [key, setKey] = React.useState('');
  const [envKey, setEnvKey] = React.useState<string>('ANTHROPIC_AUTH_TOKEN');
  const [kimiProduct, setKimiProduct] = React.useState<'kimi-code-membership' | 'kimi-api'>(
    'kimi-api',
  );
  const [saving, setSaving] = React.useState(false);

  const isEdit = mode === 'edit';
  const agentName = resolveAgentMeta(agentId).name;
  const showClaudeEnv = agentId === 'claude';

  React.useEffect(() => {
    if (!open) return;
    if (isEdit && account) {
      setLabel(account.label ?? '');
      setKey('');
      setEnvKey('ANTHROPIC_AUTH_TOKEN');
      setKimiProduct('kimi-api');
    } else {
      setLabel('');
      setKey('');
      setEnvKey('ANTHROPIC_AUTH_TOKEN');
      setKimiProduct('kimi-api');
    }
  }, [open, isEdit, account]);

  const canSave = isEdit
    ? Boolean(label.trim() || key.trim())
    : Boolean(key.trim());

  const requestClose = () => {
    if (saving) return;
    onOpenChange(false);
  };

  const save = async () => {
    setSaving(true);
    try {
      if (isEdit) {
        if (!account) throw new Error(t('connections.apiKeyDialog.missingAccount'));
        const acc = await updateApiKeyAccount(agentId, account.id, {
          label: label.trim() || null,
          key: key.trim() || null,
        });
        toast({
          title: t('connections.apiKeyDialog.updated'),
          description: acc.isCurrent && key.trim()
            ? t('connections.apiKeyDialog.updatedWrote', { label: acc.label })
            : acc.isCurrent
              ? t('connections.apiKeyDialog.updatedNameOnly', { label: acc.label })
              : t('connections.apiKeyDialog.updatedPool', { label: acc.label }),
          variant: 'success',
        });
        onSaved(acc);
      } else {
        const acc = await addApiKeyAccount(
          agentId,
          key.trim(),
          label.trim() || null,
          showClaudeEnv ? envKey : null,
          agentId === 'claude'
            ? 'anthropic'
            : agentId === 'codex'
              ? 'openai'
              : agentId === 'grok'
                ? 'xai'
                : agentId === 'kimi'
                  ? kimiProduct
                  : null,
        );
        toast({
          title: t('connections.apiKeyDialog.added'),
          description: acc.label,
          variant: 'success',
        });
        onSaved(acc);
      }
      onOpenChange(false);
    } catch (e) {
      toast({
        title: isEdit ? t('connections.apiKeyDialog.updateFailed') : t('connections.apiKeyDialog.addFailed'),
        description: String(e),
        variant: 'danger',
      });
    } finally {
      setSaving(false);
    }
  };

  const title = isEdit
    ? t('connections.apiKeyDialog.editTitle', { name: agentName })
    : t('connections.apiKeyDialog.addTitle', { name: agentName });
  const cancelButton = (
    <Button type="button" variant="outline" size="sm" onClick={requestClose} disabled={saving}>
      {t('common.cancel')}
    </Button>
  );
  const saveButton = (
    <Button disabled={!canSave || saving} onClick={() => void save()} size="sm">
      {saving
        ? t('common.saving')
        : isEdit
          ? t('connections.apiKeyDialog.saveEdit')
          : t('common.save')}
    </Button>
  );
  const headerActions = (
    <>
      {cancelButton}
      {saveButton}
    </>
  );

  const form = (
        <div className="flex flex-col gap-3">
          <label className="flex flex-col gap-1.5">
            <span className="text-xs text-muted">
              {isEdit ? t('connections.apiKeyDialog.name') : t('connections.apiKeyDialog.nameOptional')}
            </span>
            <Input
              value={label}
              onChange={(e) => setLabel(e.target.value)}
              placeholder={isEdit
                ? t('connections.apiKeyDialog.namePlaceholderEdit')
                : t('connections.apiKeyDialog.namePlaceholderAdd')}
              autoComplete="off"
            />
          </label>

          <label className="flex flex-col gap-1.5">
            <span className="text-xs text-muted">
              {t('connections.apiKeyDialog.key')}
            </span>
            <SecretInput
              value={key}
              onChange={setKey}
              placeholder={isEdit
                ? t('connections.apiKeyDialog.keyPlaceholderEdit')
                : t('connections.apiKeyDialog.keyPlaceholderAdd')}
            />
            {isEdit ? (
              <p className="text-meta text-muted">{t('connections.apiKeyDialog.keyHint')}</p>
            ) : null}
          </label>

          {showClaudeEnv && !isEdit ? (
            <label className="flex flex-col gap-1.5">
              <Hint label={t('connections.apiKeyDialog.envHint')}>
                <span className="text-xs text-muted">{t('connections.apiKeyDialog.envField')}</span>
              </Hint>
              <Select value={envKey} onValueChange={setEnvKey}>
                <SelectTrigger>
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  {CLAUDE_ENV_KEYS.map((item) => (
                    <SelectItem key={item.value} value={item.value}>
                      {item.value === 'ANTHROPIC_AUTH_TOKEN'
                        ? t('connections.apiKeyDialog.envAuthToken')
                        : t('connections.apiKeyDialog.envApiKey')}
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
            </label>
          ) : null}

          {agentId === 'kimi' && !isEdit ? (
            <label className="flex flex-col gap-1.5">
              <span className="text-xs text-muted">Kimi Key 类型</span>
              <Select value={kimiProduct} onValueChange={(value) => setKimiProduct(value as typeof kimiProduct)}>
                <SelectTrigger>
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  <SelectItem value="kimi-api">Kimi 开放平台 API Key</SelectItem>
                  <SelectItem value="kimi-code-membership">Kimi Code 会员 Key</SelectItem>
                </SelectContent>
              </Select>
            </label>
          ) : null}
        </div>
  );

  if (asPanel) {
    if (!open) return null;
    return (
      <SideInspectPanel
        title={title}
        onClose={requestClose}
        headerActions={headerActions}
        width={width}
      >
        {form}
      </SideInspectPanel>
    );
  }

  return (
    <Dialog
      open={open}
      onOpenChange={(v) => {
        if (!v) requestClose();
        else onOpenChange(true);
      }}
    >
      <DialogContent className="max-w-md">
        <DialogHeader>
          <DialogTitle>{title}</DialogTitle>
        </DialogHeader>
        {form}
        <DialogFooter>
          {headerActions}
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}

