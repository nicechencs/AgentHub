import { Link } from 'react-router-dom';
import { Card, CardContent, CardFooter } from '@/components/ui/card';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import {
  Select,
  SelectValue,
  SelectTrigger,
  SelectContent,
  SelectItem,
} from '@/components/ui/select';
import { useToast } from '@/components/ui/toast';
import { LOG_LEVEL_OPTIONS, openLogsDir, updateSettings } from '@/lib/api/settings';
import { BRIDGES_PATH } from '@/lib/bridges-path';
import type { AppSettings, LogLevel } from '@/lib/types';
import { notifyUsageSettingsChanged } from '@/lib/usage-sync';
import { dataSettingsSaveDescription } from './settings-format';
import { SettingsRow } from './settings-shared';

export function DataPanel({
  settings,
  patch,
  setSettings,
  saving,
  setSaving,
}: {
  settings: AppSettings;
  patch: (p: Partial<AppSettings>) => void;
  setSettings: (s: AppSettings) => void;
  saving: boolean;
  setSaving: (v: boolean) => void;
}) {
  const { toast } = useToast();
  return (
          <Card>
            <CardContent className="divide-y divide-border pt-1">
              <SettingsRow
                label="本机路由运行时"
                description="永远可打开"
                descriptionTip="管本机转发进程。侧栏只在有本机路由时出现；这里始终可找回。"
              >
                <Link
                  to={BRIDGES_PATH}
                  className="inline-flex h-7 items-center justify-center rounded-btn border border-border bg-transparent px-2.5 text-xs font-medium text-secondary transition-colors hover:bg-hover hover:text-primary"
                >
                  打开
                </Link>
              </SettingsRow>
              <SettingsRow
                label="数据目录"
                description="只读"
                descriptionTip="配置快照、备份与统计数据存放位置（桌面端不可改）。"
              >
                <Input
                  className="w-full font-mono text-xs"
                  value={settings.dataDir}
                  readOnly
                  title={settings.dataDir}
                />
              </SettingsRow>
              <SettingsRow
                label="日志级别"
                description="下次启动生效"
                descriptionTip="写入数据目录 logs 的详细程度。"
              >
                <Select
                  value={settings.logLevel}
                  onValueChange={(v) => patch({ logLevel: v as LogLevel })}
                >
                  <SelectTrigger className="w-full">
                    <SelectValue />
                  </SelectTrigger>
                  <SelectContent>
                    {LOG_LEVEL_OPTIONS.map((opt) => (
                      <SelectItem key={opt.value} value={opt.value}>
                        {opt.label}
                      </SelectItem>
                    ))}
                  </SelectContent>
                </Select>
              </SettingsRow>
              <SettingsRow
                label="日志保留"
                description="天；默认 14"
                descriptionTip="按日日志超过天数后，启动时自动清理（1–365）。"
              >
                <Input
                  type="number"
                  min={1}
                  max={365}
                  className="w-20"
                  value={settings.logRetentionDays}
                  onChange={(e) => {
                    const n = parseInt(e.target.value, 10);
                    if (Number.isNaN(n)) {
                      patch({ logRetentionDays: 14 });
                      return;
                    }
                    patch({ logRetentionDays: Math.min(365, Math.max(1, n)) });
                  }}
                />
              </SettingsRow>
              <SettingsRow
                label="日志目录"
                description="按日文件"
                descriptionTip="文件名 agenthub.YYYY-MM-DD；排查问题时可打开此目录。"
              >
                <Input
                  className="w-full font-mono text-xs"
                  value={settings.logsDir}
                  readOnly
                  title={settings.logsDir}
                />
                <Button
                  size="sm"
                  variant="outline"
                  onClick={() => {
                    void (async () => {
                      try {
                        const p = await openLogsDir();
                        toast({ title: '已打开日志目录', description: p, variant: 'success' });
                      } catch (e) {
                        toast({
                          title: '打开失败',
                          description: String(e),
                          variant: 'danger',
                        });
                      }
                    })();
                  }}
                >
                  打开
                </Button>
              </SettingsRow>
              <SettingsRow
                label="用量采集间隔"
                description="分钟；0=仅手动"
                descriptionTip="App 前台时按间隔自动增量采集，后台暂停。总览显示上次/下次同步。"
              >
                <Input
                  type="number"
                  min={0}
                  max={24 * 60}
                  className="w-20"
                  value={settings.usageCollectIntervalMin}
                  onChange={(e) => {
                    const n = parseInt(e.target.value, 10);
                    if (Number.isNaN(n)) {
                      patch({ usageCollectIntervalMin: 0 });
                      return;
                    }
                    patch({ usageCollectIntervalMin: Math.min(24 * 60, Math.max(0, n)) });
                  }}
                />
                <Link
                  to="/?section=usage"
                  className="inline-flex h-7 items-center justify-center rounded-btn border border-border bg-transparent px-2.5 text-xs font-medium text-secondary transition-colors hover:bg-hover hover:text-primary"
                >
                  查看
                </Link>
              </SettingsRow>
            </CardContent>
            <CardFooter className="flex flex-col items-stretch gap-2 sm:flex-row sm:items-center sm:justify-between">
              <p className="text-xs text-muted">
                日志级别与保留天数写入本机配置；级别变更需重启应用后生效。
              </p>
              <Button
                disabled={saving}
                onClick={() =>
                  void (async () => {
                    setSaving(true);
                    try {
                      const next = await updateSettings({
                        logLevel: settings.logLevel,
                        logRetentionDays: settings.logRetentionDays,
                        usageCollectIntervalMin: settings.usageCollectIntervalMin,
                      });
                      setSettings(next);
                      notifyUsageSettingsChanged();
                      toast({
                        title: '数据设置已保存',
                        description: dataSettingsSaveDescription(next),
                        variant: 'success',
                      });
                    } catch (e) {
                      toast({ title: '保存失败', description: String(e), variant: 'danger' });
                    } finally {
                      setSaving(false);
                    }
                  })()
                }
              >
                {saving ? '保存中…' : '保存'}
              </Button>
            </CardFooter>
          </Card>
  );
}
