import { AlertTriangle, CheckCircle2 } from 'lucide-react';
import {
  Dialog,
  DialogContent,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog';
import { Button } from '@/components/ui/button';
import type { SwitchPreview } from '@/lib/types';

/**
 * 供应商/账号切换确认对话框(docs/ui-design.md §4.3):
 * backfill 摘要 + 备份位置 + 运行中进程警告 三要素。
 */
export function SwitchConfirmDialog({
  open,
  onOpenChange,
  targetName,
  preview,
  loading,
  onConfirm,
}: {
  open: boolean;
  onOpenChange: (v: boolean) => void;
  targetName: string;
  /** 未提供时用默认三要素文案 */
  preview?: SwitchPreview;
  loading?: boolean;
  onConfirm: () => void;
}) {
  const p: SwitchPreview = preview ?? {
    backfillSummary: '当前生效配置将先保存回连接池',
    backupPath: '~/.agenthub/backups/live/',
  };
  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="max-w-md">
        <DialogHeader>
          <DialogTitle>切换到「{targetName}」？</DialogTitle>
        </DialogHeader>
        <div className="flex flex-col gap-2.5 text-sm">
          <div className="flex items-start gap-2">
            <CheckCircle2 className="mt-0.5 h-4 w-4 shrink-0 text-success" />
            <span className="text-secondary">{p.backfillSummary}</span>
          </div>
          <div className="flex items-start gap-2">
            <CheckCircle2 className="mt-0.5 h-4 w-4 shrink-0 text-success" />
            <span className="text-secondary">
              切换前备份到 <code className="font-mono text-xs">{p.backupPath}</code>
            </span>
          </div>
          {p.processWarning && (
            <div className="flex items-start gap-2">
              <AlertTriangle className="mt-0.5 h-4 w-4 shrink-0 text-warning" />
              <span className="text-warning">{p.processWarning}</span>
            </div>
          )}
        </div>
        <DialogFooter>
          <Button variant="outline" onClick={() => onOpenChange(false)}>
            取消
          </Button>
          <Button disabled={loading} onClick={onConfirm}>
            {loading ? '切换中…' : '确认切换'}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
