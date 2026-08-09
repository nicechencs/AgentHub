import * as React from 'react';
import { Loader2, RotateCcw, Trash2 } from 'lucide-react';
import { Button } from '@/components/ui/button';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog';
import { useToast } from '@/components/ui/toast';
import type { ConnectionTrashItem } from '@/lib/backend/contracts';
import {
  listConnectionTrash,
  permanentlyDeleteConnectionTrash,
  restoreConnectionTrash,
} from '@/lib/api/trash';
import { AGENT_MAP } from '@/config/agents';
import type { AgentId } from '@/lib/types';

function dateLabel(value: string): string {
  const isoLike = value.replace(' ', 'T');
  const normalized = /(?:Z|[+-]\d{2}:?\d{2})$/i.test(isoLike) ? isoLike : `${isoLike}Z`;
  const date = new Date(normalized);
  return Number.isNaN(date.getTime()) ? value : date.toLocaleString();
}

export function ConnectionTrashButton({
  agentId,
  onChanged,
}: {
  agentId?: AgentId;
  onChanged?: () => void;
}) {
  const { toast } = useToast();
  const [open, setOpen] = React.useState(false);
  const [items, setItems] = React.useState<ConnectionTrashItem[]>([]);
  const [loading, setLoading] = React.useState(false);
  const [busyId, setBusyId] = React.useState<string | null>(null);

  const load = React.useCallback(async () => {
    setLoading(true);
    try {
      setItems(await listConnectionTrash(agentId));
    } catch (error) {
      toast({
        title: '无法读取回收站',
        description: error instanceof Error ? error.message : String(error),
        variant: 'danger',
      });
    } finally {
      setLoading(false);
    }
  }, [agentId, toast]);

  const restore = async (item: ConnectionTrashItem) => {
    setBusyId(item.id);
    try {
      await restoreConnectionTrash(item.id);
      setItems((current) => current.filter((row) => row.id !== item.id));
      toast({ title: '已恢复连接', description: '已恢复到连接列表，未自动写入本机配置。' });
      onChanged?.();
    } catch (error) {
      toast({
        title: '恢复失败',
        description: error instanceof Error ? error.message : String(error),
        variant: 'danger',
      });
    } finally {
      setBusyId(null);
    }
  };

  const permanentlyDelete = async (item: ConnectionTrashItem) => {
    if (!window.confirm(`确定永久删除“${item.label}”吗？此操作不可恢复。`)) return;
    setBusyId(item.id);
    try {
      await permanentlyDeleteConnectionTrash(item.id);
      setItems((current) => current.filter((row) => row.id !== item.id));
      toast({ title: '已永久删除' });
    } catch (error) {
      toast({
        title: '永久删除失败',
        description: error instanceof Error ? error.message : String(error),
        variant: 'danger',
      });
    } finally {
      setBusyId(null);
    }
  };

  return (
    <>
      <div className="fixed bottom-4 right-4 z-20">
        <Button
          type="button"
          variant="outline"
          size="sm"
          className="gap-2 rounded-full shadow-lg"
          onClick={() => {
            setOpen(true);
            void load();
          }}
        >
          <Trash2 className="h-4 w-4" aria-hidden />
          回收站
        </Button>
      </div>

      <Dialog open={open} onOpenChange={setOpen}>
        <DialogContent className="max-w-xl">
          <DialogHeader>
            <DialogTitle>认证信息回收站</DialogTitle>
            <DialogDescription>
              删除的官方登录与 API Key 会保留 30 天。恢复只回到 AgentHub 列表，不会自动写入本机配置。
            </DialogDescription>
          </DialogHeader>

          <div className="max-h-[55vh] space-y-2 overflow-y-auto py-2">
            {loading ? (
              <div className="flex items-center justify-center gap-2 py-8 text-sm text-muted-foreground">
                <Loader2 className="h-4 w-4 animate-spin" />
                正在读取回收站…
              </div>
            ) : items.length === 0 ? (
              <p className="py-8 text-center text-sm text-muted-foreground">回收站为空</p>
            ) : (
              items.map((item) => {
                const agentName = AGENT_MAP[item.agentId]?.name ?? item.agentId;
                const kindLabel =
                  item.kind === 'account' && item.account?.kind === 'oauth'
                    ? '官方登录'
                    : 'API Key';
                return (
                  <div
                    key={item.id}
                    className="flex items-center justify-between gap-3 rounded-lg border p-3"
                  >
                    <div className="min-w-0">
                      <p className="truncate text-sm font-medium">{item.label}</p>
                      <p className="text-xs text-muted-foreground">
                        {agentName} · {kindLabel} · 删除于 {dateLabel(item.deletedAt)}
                      </p>
                      <p className="text-xs text-muted-foreground">
                        {item.wasCurrent ? '删除前为当前连接 · ' : ''}保留至 {dateLabel(item.expiresAt)}
                      </p>
                    </div>
                    <div className="flex shrink-0 gap-2">
                      <Button
                        type="button"
                        variant="outline"
                        size="sm"
                        disabled={busyId === item.id}
                        onClick={() => void restore(item)}
                      >
                        <RotateCcw className="mr-1 h-3.5 w-3.5" />
                        恢复
                      </Button>
                      <Button
                        type="button"
                        variant="ghost"
                        size="sm"
                        className="text-destructive"
                        disabled={busyId === item.id}
                        onClick={() => void permanentlyDelete(item)}
                      >
                        永久删除
                      </Button>
                    </div>
                  </div>
                );
              })
            )}
          </div>

          <DialogFooter>
            <Button type="button" variant="outline" onClick={() => void load()} disabled={loading}>
              刷新
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </>
  );
}
