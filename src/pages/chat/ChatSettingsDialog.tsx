import { FolderOpen } from 'lucide-react';
import { Button } from '@/components/ui/button';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog';
import { Input } from '@/components/ui/input';
import { Switch } from '@/components/ui/switch';
import { Tip } from '@/components/ui/tooltip';
import type { Conversation } from '@/lib/types';

export function ChatSettingsDialog({
  open,
  onOpenChange,
  active,
  dangerConfirm,
  onDangerConfirmChange,
  onPatch,
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  active: Conversation | null;
  dangerConfirm: boolean;
  onDangerConfirmChange: (open: boolean) => void;
  onPatch: (patch: { cwd?: string | null; allowDangerous?: boolean }) => void;
}) {
  return (
    <>
      <Dialog open={open} onOpenChange={onOpenChange}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>会话设置</DialogTitle>
            <DialogDescription>工作目录与自动批准</DialogDescription>
          </DialogHeader>
          {active && (
            <div className="space-y-4 py-2">
              <div>
                <label className="mb-1.5 flex items-center gap-1.5 text-meta text-muted">
                  <FolderOpen className="h-3.5 w-3.5" />
                  工作目录
                </label>
                <Input
                  key={active.id}
                  placeholder="例如 D:\\projects\\demo"
                  defaultValue={active.cwd ?? ''}
                  onBlur={(e) => {
                    const v = e.target.value.trim();
                    onPatch({ cwd: v || null });
                  }}
                />
              </div>
              <label className="flex items-center justify-between gap-3 text-body">
                <span>
                  <span className="block font-medium">自动批准</span>
                  <Tip className="text-meta text-muted" label="关闭时 CLI 遇审批可能等到超时">
                    跳过工具确认
                  </Tip>
                </span>
                <Switch
                  checked={active.allowDangerous}
                  onCheckedChange={(checked) => {
                    if (checked) {
                      onDangerConfirmChange(true);
                      return;
                    }
                    onPatch({ allowDangerous: false });
                  }}
                />
              </label>
            </div>
          )}
          <DialogFooter>
            <Button variant="secondary" onClick={() => onOpenChange(false)}>
              完成
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      <Dialog open={dangerConfirm} onOpenChange={onDangerConfirmChange}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>开启自动批准？</DialogTitle>
            <DialogDescription>
              开启后将跳过工具确认，Agent 可直接改文件、执行命令。仅在信任当前工作目录时开启。
            </DialogDescription>
          </DialogHeader>
          <DialogFooter>
            <Button variant="secondary" onClick={() => onDangerConfirmChange(false)}>
              取消
            </Button>
            <Button
              variant="danger"
              onClick={() => {
                onDangerConfirmChange(false);
                onPatch({ allowDangerous: true });
              }}
            >
              确认开启
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </>
  );
}
