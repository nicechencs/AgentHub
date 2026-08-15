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

export type AgentCardConfirmKind = 'program' | 'config' | 'force-upgrade' | null;

export type AgentCardDialogsProps = {
  agentName: string;
  confirmDialog: AgentCardConfirmKind;
  confirmName: string;
  onConfirmNameChange: (v: string) => void;
  uninstalling: boolean;
  busy: boolean;
  updateState: string | undefined;
  onClose: () => void;
  onUninstall: (deleteConfig: boolean) => void;
  onConfirmForceUpgrade: () => void;
};

export function AgentCardDialogs({
  agentName,
  confirmDialog,
  confirmName,
  onConfirmNameChange,
  uninstalling,
  busy,
  updateState,
  onClose,
  onUninstall,
  onConfirmForceUpgrade,
}: AgentCardDialogsProps) {
  return (
    <>
      <Dialog open={confirmDialog === 'program'} onOpenChange={(o) => !o && onClose()}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>卸载 {agentName}？</DialogTitle>
            <DialogDescription>
              只卸程序，不卸 Node 等共享环境。
            </DialogDescription>
          </DialogHeader>
          <DialogFooter>
            <Button variant="outline" onClick={() => onClose()} disabled={uninstalling}>
              取消
            </Button>
            <Button variant="danger" onClick={() => onUninstall(false)} disabled={uninstalling}>
              {uninstalling ? '卸载中...' : '确认卸载'}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      <Dialog
        open={confirmDialog === 'force-upgrade'}
        onOpenChange={(o) => !o && onClose()}
      >
        <DialogContent>
          <DialogHeader>
            <DialogTitle>强制升级 {agentName}？</DialogTitle>
            <DialogDescription>
              {updateState === 'up_to_date'
                ? '当前检测为已是最新版本。强制升级将按已装渠道重新安装 / 重跑官方脚本。'
                : '未能确认是否有新版本。强制升级将按已装渠道重新安装 / 重跑官方脚本。'}
            </DialogDescription>
          </DialogHeader>
          <DialogFooter>
            <Button variant="outline" onClick={() => onClose()} disabled={busy}>
              取消
            </Button>
            <Button
              variant="default"
              disabled={busy}
              onClick={() => {
                onClose();
                onConfirmForceUpgrade();
              }}
            >
              确认强制升级
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      <Dialog
        open={confirmDialog === 'config'}
        onOpenChange={(o) => {
          if (!o) {
            onClose();
            onConfirmNameChange('');
          }
        }}
      >
        <DialogContent>
          <DialogHeader>
            <DialogTitle>卸载并删除配置？</DialogTitle>
            <DialogDescription className="text-danger">
              先备份再删配置；不卸 Node 等共享环境。
            </DialogDescription>
          </DialogHeader>
          <div className="text-sm text-secondary">
            输入 <span className="font-mono font-medium text-primary">{agentName}</span> 确认：
          </div>
          <Input
            value={confirmName}
            onChange={(e) => onConfirmNameChange(e.target.value)}
            placeholder={agentName}
            className="mt-2"
            disabled={uninstalling}
          />
          <DialogFooter>
            <Button variant="outline" onClick={() => onClose()} disabled={uninstalling}>
              取消
            </Button>
            <Button
              variant="danger"
              onClick={() => onUninstall(true)}
              disabled={uninstalling || confirmName !== agentName}
            >
              {uninstalling ? '卸载中...' : '卸载并删除配置'}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </>
  );
}
