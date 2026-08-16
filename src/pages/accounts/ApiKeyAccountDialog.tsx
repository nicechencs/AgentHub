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
import { useToast } from '@/components/ui/toast';
import { addApiKeyAccount, updateApiKeyAccount } from '@/lib/api/account';
import { resolveAgentMeta } from '@/config/agents';
import type { Account, AgentId } from '@/lib/types';

/** Claude live apply 时写入 settings.json 的 env 字段 */
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
}: {
  agentId: AgentId;
  open: boolean;
  onOpenChange: (v: boolean) => void;
  mode?: ApiKeyDialogMode;
  /** 编辑模式目标账号 */
  account?: Account | null;
  onSaved: (acc: Account) => void;
}) {
  const { toast } = useToast();
  const [label, setLabel] = React.useState('');
  const [key, setKey] = React.useState('');
  const [envKey, setEnvKey] = React.useState<string>('ANTHROPIC_AUTH_TOKEN');
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
    } else {
      setLabel('');
      setKey('');
      setEnvKey('ANTHROPIC_AUTH_TOKEN');
    }
  }, [open, isEdit, account]);

  const canSave = isEdit
    ? Boolean(label.trim() || key.trim())
    : Boolean(key.trim());

  const save = async () => {
    setSaving(true);
    try {
      if (isEdit) {
        if (!account) throw new Error('缺少待编辑账号');
        const acc = await updateApiKeyAccount(agentId, account.id, {
          label: label.trim() || null,
          key: key.trim() || null,
        });
        toast({
          title: 'API Key 账号已更新',
          description: acc.isCurrent && key.trim()
            ? `${acc.label} · 已写入本机配置`
            : acc.isCurrent
              ? `${acc.label} · 仅更新名称，本机配置未改`
              : `${acc.label} · 已保存到连接池，切换后才会写入本机`,
          variant: 'success',
        });
        onSaved(acc);
      } else {
        const acc = await addApiKeyAccount(
          agentId,
          key.trim(),
          label.trim() || null,
          showClaudeEnv ? envKey : null,
        );
        toast({
          title: 'API Key 账号已添加',
          description: acc.label,
          variant: 'success',
        });
        onSaved(acc);
      }
      onOpenChange(false);
    } catch (e) {
      toast({
        title: isEdit ? '更新失败' : '添加失败',
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
            {isEdit ? '编辑 API Key' : '添加 API Key'} — {agentName}
          </DialogTitle>
          <DialogDescription>
            {isEdit
              ? '可改名称；API Key 留空则保留原密钥。当前连接改密钥会写入本机。'
              : '保存后可切换到本机生效。密钥默认脱敏。'}
          </DialogDescription>
        </DialogHeader>

        <div className="flex flex-col gap-3">
          <label className="flex flex-col gap-1.5">
            <span className="text-xs text-muted">
              名称{isEdit ? '' : '（可选）'}
            </span>
            <Input
              value={label}
              onChange={(e) => setLabel(e.target.value)}
              placeholder={isEdit ? '账号显示名' : '例如：工作号 / 个人号'}
              autoComplete="off"
            />
          </label>

          <label className="flex flex-col gap-1.5">
            <span className="text-xs text-muted">
              API Key{isEdit ? '（留空保留原密钥）' : ''}
            </span>
            <SecretInput
              value={key}
              onChange={setKey}
              placeholder={isEdit ? '输入新密钥以替换…' : 'sk-…'}
            />
          </label>

          {showClaudeEnv && !isEdit && (
            <label className="flex flex-col gap-1.5">
              <span className="text-xs text-muted">写入 settings 的字段</span>
              <Select value={envKey} onValueChange={setEnvKey}>
                <SelectTrigger>
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  {CLAUDE_ENV_KEYS.map((item) => (
                    <SelectItem key={item.value} value={item.value}>
                      {item.label}
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
              <span className="text-meta text-muted">
                切换生效时写入 Claude 配置中的该环境变量字段。
              </span>
            </label>
          )}
        </div>

        <DialogFooter>
          <Button variant="outline" onClick={() => onOpenChange(false)}>
            取消
          </Button>
          <Button disabled={!canSave || saving} onClick={save}>
            {saving ? '保存中…' : isEdit ? '保存修改' : '保存'}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}

