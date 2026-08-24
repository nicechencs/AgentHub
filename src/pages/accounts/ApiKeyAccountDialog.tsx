import * as React from 'react';
import {
  Dialog,
  DialogContent,
  DialogDescription,
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

const CLAUDE_DEFAULT_ENV = 'ANTHROPIC_AUTH_TOKEN';

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
}: {
  agentId: AgentId;
  open: boolean;
  onOpenChange: (v: boolean) => void;
  mode?: ApiKeyDialogMode;
  /** 编辑模式目标账号 */
  account?: Account | null;
  onSaved: (acc: Account) => void;
}) {
  const { t } = useI18n();
  const { toast } = useToast();
  const [label, setLabel] = React.useState('');
  const [key, setKey] = React.useState('');
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
      setKimiProduct('kimi-api');
    } else {
      setLabel('');
      setKey('');
      setKimiProduct('kimi-api');
    }
  }, [open, isEdit, account]);

  const canSave = isEdit
    ? Boolean(label.trim() || key.trim())
    : Boolean(key.trim());

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
          showClaudeEnv ? CLAUDE_DEFAULT_ENV : null,
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

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="max-w-md">
        <DialogHeader>
          <DialogTitle>
            {isEdit
              ? t('connections.apiKeyDialog.editTitle', { name: agentName })
              : t('connections.apiKeyDialog.addTitle', { name: agentName })}
          </DialogTitle>
          <DialogDescription>
            {isEdit
              ? t('connections.apiKeyDialog.editDesc')
              : t('connections.apiKeyDialog.addDesc')}
          </DialogDescription>
        </DialogHeader>

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
            <Hint label={isEdit ? t('connections.apiKeyDialog.keyKeep') : undefined}>
              <span className="text-xs text-muted">
                {t('connections.apiKeyDialog.key')}
              </span>
            </Hint>
            <SecretInput
              value={key}
              onChange={setKey}
              placeholder={isEdit
                ? t('connections.apiKeyDialog.keyPlaceholderEdit')
                : t('connections.apiKeyDialog.keyPlaceholderAdd')}
            />
          </label>

          {agentId === 'kimi' && !isEdit && (
            <label className="flex flex-col gap-1.5">
              <span className="text-xs text-muted">Kimi Key 类型</span>
              <Select value={kimiProduct} onValueChange={(value) => setKimiProduct(value as typeof kimiProduct)}>
                <SelectTrigger>
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  <SelectItem value="kimi-api">开放平台</SelectItem>
                  <SelectItem value="kimi-code-membership">Code 会员</SelectItem>
                </SelectContent>
              </Select>
            </label>
          )}
        </div>

        <DialogFooter>
          <Button variant="outline" onClick={() => onOpenChange(false)}>
            {t('common.cancel')}
          </Button>
          <Button disabled={!canSave || saving} onClick={save}>
            {saving
              ? t('common.saving')
              : isEdit
                ? t('connections.apiKeyDialog.saveEdit')
                : t('common.save')}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}

