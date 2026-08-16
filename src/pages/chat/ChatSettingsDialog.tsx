import { useEffect, useState } from 'react';
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
import { useToast } from '@/components/ui/toast';
import { Tip } from '@/components/ui/tooltip';
import { pickDirectory } from '@/lib/api/settings';
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
  const { toast } = useToast();
  const [cwdDraft, setCwdDraft] = useState(active?.cwd ?? '');
  const [picking, setPicking] = useState(false);

  useEffect(() => {
    setCwdDraft(active?.cwd ?? '');
  }, [active?.id, active?.cwd]);

  function commitCwd(raw: string) {
    const v = raw.trim();
    onPatch({ cwd: v || null });
  }

  async function handleBrowse() {
    setPicking(true);
    try {
      const picked = await pickDirectory({
        title: '选择工作目录',
        defaultPath: cwdDraft || active?.cwd || null,
      });
      if (picked) {
        setCwdDraft(picked);
        onPatch({ cwd: picked });
      }
    } catch (e) {
      toast({
        title: '无法选择目录',
        description: e instanceof Error ? e.message : String(e),
        variant: 'danger',
      });
    } finally {
      setPicking(false);
    }
  }

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
                <div className="flex gap-2">
                  <Input
                    value={cwdDraft}
                    placeholder="选择本机目录，或粘贴完整路径"
                    aria-label="工作目录"
                    onChange={(e) => setCwdDraft(e.target.value)}
                    onBlur={(e) => commitCwd(e.target.value)}
                  />
                  <Button
                    type="button"
                    variant="outline"
                    disabled={picking}
                    onClick={() => void handleBrowse()}
                  >
                    {picking ? '选择中…' : '选择目录'}
                  </Button>
                </div>
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
