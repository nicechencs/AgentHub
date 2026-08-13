// Settings 设置页(docs/ui-design.md §4.8)
// Tabs:常规 / 安全 / 数据 / 备份 / 关于；tab 与 ?tab= URL 同步。
// 常规/数据草稿态编辑后点 [保存]；备份分区操作即时生效，无底部保存。
import { useEffect, useState } from 'react';
import type { ReactNode } from 'react';
import { ExternalLink } from 'lucide-react';
import { Link, useSearchParams } from 'react-router-dom';
import { PageHeader } from '@/components/layout/PageHeader';
import { Tabs, TabsList, TabsTrigger, TabsContent } from '@/components/ui/tabs';
import { Card, CardContent, CardFooter } from '@/components/ui/card';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Badge } from '@/components/ui/badge';
import { Switch } from '@/components/ui/switch';
import { Skeleton } from '@/components/ui/skeleton';
import {
  Select,
  SelectValue,
  SelectTrigger,
  SelectContent,
  SelectItem,
} from '@/components/ui/select';
import { useToast } from '@/components/ui/toast';
import { Tip } from '@/components/ui/tooltip';
import { ErrorState } from '@/components/shared/ErrorState';
import { StatusPin } from '@/components/shared/StatusPin';
import { useTheme } from '@/components/shared/ThemeProvider';
import {
  getSettings,
  LOG_LEVEL_OPTIONS,
  openLogsDir,
  updateSettings,
} from '@/lib/api/settings';
import {
  setAppUpdateAvailable,
  useAppUpdateAvailable,
} from '@/app/runtime';
import {
  checkForUpdate,
  downloadAndInstallUpdate,
  getAppVersion,
  isUpdateAvailable,
  type UpdateInfo,
} from '@/lib/api/update';
import { openExternalLink } from '@/lib/open-external';
import { invalidateSkills } from '@/lib/hooks/useSkills';
import type { AppSettings, LogLevel, SkillMarketSource } from '@/lib/types';
import { notifyUsageSettingsChanged } from '@/lib/usage-sync';
import { BackupsPanel } from './BackupsPanel';

/** Project homepage on GitHub (releases, issues, source). */
const GITHUB_REPO_URL = 'https://github.com/nicechencs/AgentHub';

const SKILL_MARKET_OPTIONS: { value: SkillMarketSource; label: string }[] = [
  { value: 'auto', label: '自动（不通则切换）' },
  { value: 'skills.sh', label: 'skills.sh' },
  { value: 'skillhub.cn', label: 'skillhub.cn' },
];

const SETTINGS_TABS = ['general', 'security', 'data', 'backups', 'about'] as const;
type SettingsTab = (typeof SETTINGS_TABS)[number];

function parseSettingsTab(raw: string | null): SettingsTab {
  if (raw && (SETTINGS_TABS as readonly string[]).includes(raw)) {
    return raw as SettingsTab;
  }
  return 'general';
}

function skillMarketLabel(source: SkillMarketSource | undefined): string {
  const value = source ?? 'auto';
  return SKILL_MARKET_OPTIONS.find((opt) => opt.value === value)?.label ?? '自动（不通则切换）';
}

/** 常规区保存成功摘要：覆盖本区实际写入项，避免只提技能市场。 */
function generalSettingsSaveDescription(s: AppSettings): string {
  const tray = s.closeToTray ? '关闭时到托盘' : '关闭时直接退出';
  return `${tray} · 技能市场 ${skillMarketLabel(s.skillMarketSource)}`;
}

/** 数据区保存成功摘要：区分立即生效与需重启项。 */
function dataSettingsSaveDescription(s: AppSettings): string {
  const usage =
    s.usageCollectIntervalMin <= 0
      ? '用量仅手动采集'
      : `用量每 ${s.usageCollectIntervalMin} 分钟自动采集`;
  return `${usage}；日志级别 ${s.logLevel} 下次启动生效（保留 ${s.logRetentionDays} 天）`;
}

/** 表单行：左侧标签 + 短说明；细节用 descriptionTip 悬停展示 */
function Row({
  label,
  description,
  descriptionTip,
  children,
}: {
  label: string;
  description?: string;
  descriptionTip?: string;
  children: ReactNode;
}) {
  return (
    <div className="flex items-center justify-between gap-6 py-3">
      <div className="min-w-0">
        <p className="text-sm">{label}</p>
        {description &&
          (descriptionTip ? (
            <Tip className="mt-0.5 block text-xs text-muted" label={descriptionTip}>
              {description}
            </Tip>
          ) : (
            <p className="mt-0.5 text-xs text-muted">{description}</p>
          ))}
      </div>
      <div className="flex w-48 shrink-0 items-center justify-end gap-2">{children}</div>
    </div>
  );
}

function SettingsSkeleton() {
  return (
    <div className="space-y-4">
      <Skeleton className="h-9 w-72" />
      <Card>
        <CardContent className="divide-y divide-border pt-1">
          {[0, 1, 2, 3].map((i) => (
            <div key={i} className="flex items-center justify-between py-4">
              <div className="space-y-2">
                <Skeleton className="h-4 w-24" />
                <Skeleton className="h-3 w-40" />
              </div>
              <Skeleton className="h-8 w-40" />
            </div>
          ))}
        </CardContent>
      </Card>
    </div>
  );
}

export default function SettingsPage({
  onCheckUpdate,
}: {
  /** Optional shared check from App UpdatePrompt (opens global dialog when found). */
  onCheckUpdate?: () => Promise<UpdateInfo | null>;
} = {}) {
  const { toast } = useToast();
  const { setTheme } = useTheme();
  const [searchParams, setSearchParams] = useSearchParams();
  const tab = parseSettingsTab(searchParams.get('tab'));

  const [settings, setSettings] = useState<AppSettings | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<unknown>(null);
  const [saving, setSaving] = useState(false);

  // 检查 / 安装更新（pending 来自全局 store，启动检查后关于页与导航同步可见）
  const [checking, setChecking] = useState(false);
  const [installing, setInstalling] = useState(false);
  const pendingUpdate = useAppUpdateAvailable();

  const load = async () => {
    setLoading(true);
    setError(null);
    try {
      const s = await getSettings();
      // Prefer shell package version when available (Tauri / mock update port).
      try {
        const v = await getAppVersion();
        if (v) s.appVersion = v;
      } catch {
        // keep settings.appVersion fallback
      }
      setSettings(s);
    } catch (e) {
      setError(e);
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    void load();
  }, []);

  const setTab = (next: string) => {
    const value = parseSettingsTab(next);
    setSearchParams({ tab: value }, { replace: true });
  };

  const patch = (p: Partial<AppSettings>) =>
    setSettings((prev) => (prev ? { ...prev, ...p } : prev));

  const checkUpdate = () => {
    void (async () => {
      setChecking(true);
      try {
        if (onCheckUpdate) {
          const found = await onCheckUpdate();
          // UpdatePrompt already publishes to the store; keep local toast for “up to date”.
          if (!found) {
            toast({ title: '已是最新版本', variant: 'success' });
          }
          return;
        }
        if (!(await isUpdateAvailable())) {
          toast({
            title: '无法检查更新',
            description: '仅桌面端支持自动更新',
            variant: 'danger',
          });
          return;
        }
        const found = await checkForUpdate();
        setAppUpdateAvailable(found);
        if (!found) {
          toast({ title: '已是最新版本', variant: 'success' });
        } else {
          toast({
            title: `发现新版本 v${found.version}`,
            description: '可点击「一键更新」安装',
            variant: 'success',
          });
        }
      } catch (e) {
        toast({
          title: '检查更新失败',
          description: e instanceof Error ? e.message : String(e),
          variant: 'danger',
        });
      } finally {
        setChecking(false);
      }
    })();
  };

  const installUpdate = () => {
    void (async () => {
      setInstalling(true);
      try {
        await downloadAndInstallUpdate();
        setAppUpdateAvailable(null);
        toast({
          title: '更新已安装',
          description: '请手动重启应用以完成更新',
          variant: 'success',
        });
      } catch (e) {
        toast({
          title: '更新失败',
          description: e instanceof Error ? e.message : String(e),
          variant: 'danger',
        });
      } finally {
        setInstalling(false);
      }
    })();
  };

  if (loading) {
    return (
      <div>
        <PageHeader
          title="设置"
          description="偏好与数据"
          descriptionTip="主题、安全展示、日志、用量采集间隔与备份。"
        />
        <SettingsSkeleton />
      </div>
    );
  }

  if (error || !settings) {
    return (
      <div>
        <PageHeader
          title="设置"
          description="偏好与数据"
          descriptionTip="主题、安全展示、日志、用量采集间隔与备份。"
        />
        <ErrorState error={error ?? new Error('设置数据为空')} onRetry={() => void load()} />
      </div>
    );
  }

  const isKeyring = settings.credentialStore === 'keyring';

  return (
    <div>
      <PageHeader
        title="设置"
        description="偏好与数据"
        descriptionTip="主题、安全展示、日志、用量采集间隔与备份。"
      />

      <Tabs value={tab} onValueChange={setTab}>
        <TabsList>
          <TabsTrigger value="general">常规</TabsTrigger>
          <TabsTrigger value="security">安全</TabsTrigger>
          <TabsTrigger value="data">数据</TabsTrigger>
          <TabsTrigger value="backups">备份</TabsTrigger>
          <TabsTrigger value="about" className="gap-1.5">
            关于
            {pendingUpdate && (
              <StatusPin
                tone="warning"
                label={`可更新至 v${pendingUpdate.version}`}
                className="shrink-0"
              />
            )}
          </TabsTrigger>
        </TabsList>

        {/* 常规 */}
        <TabsContent value="general">
          <Card>
            <CardContent className="divide-y divide-border pt-1">
              <Row label="语言" description="界面语言">
                <Select
                  value={settings.language}
                  onValueChange={(v) => patch({ language: v as AppSettings['language'] })}
                >
                  <SelectTrigger className="w-full">
                    <SelectValue />
                  </SelectTrigger>
                  <SelectContent>
                    <SelectItem value="zh">中文</SelectItem>
                    <SelectItem value="en">English</SelectItem>
                  </SelectContent>
                </Select>
              </Row>
              <Row label="主题" description="浅色 / 深色 / 跟随系统">
                <Select
                  value={settings.theme}
                  onValueChange={(v) => {
                    const theme = v as AppSettings['theme'];
                    patch({ theme });
                    setTheme(theme);
                  }}
                >
                  <SelectTrigger className="w-full">
                    <SelectValue />
                  </SelectTrigger>
                  <SelectContent>
                    <SelectItem value="light">浅色</SelectItem>
                    <SelectItem value="dark">深色</SelectItem>
                    <SelectItem value="system">跟随系统</SelectItem>
                  </SelectContent>
                </Select>
              </Row>
              <Row
                label="开机自启"
                description="登录后启动"
                descriptionTip="写入操作系统登录项（Windows 启动项 / macOS Login Item）。保存「外观与行为」后生效。"
              >
                <Switch
                  checked={settings.autoStart}
                  onCheckedChange={(v) => patch({ autoStart: v })}
                />
              </Row>
              <Row
                label="关闭到托盘"
                description="关窗不退出"
                descriptionTip="点击关闭按钮后隐藏到系统托盘，进程保持运行。Windows 可从托盘图标恢复；macOS 可从菜单栏托盘或 Dock 图标恢复。"
              >
                <Switch
                  checked={settings.closeToTray}
                  onCheckedChange={(v) => patch({ closeToTray: v })}
                />
              </Row>
              <Row
                label="技能市场"
                description="远程技能源"
                descriptionTip="自动：优先 skills.sh，网络不可达时回退 skillhub.cn。也可固定只用其一。"
              >
                <Select
                  value={settings.skillMarketSource ?? 'auto'}
                  onValueChange={(v) =>
                    patch({ skillMarketSource: v as AppSettings['skillMarketSource'] })
                  }
                >
                  <SelectTrigger className="w-full">
                    <SelectValue />
                  </SelectTrigger>
                  <SelectContent>
                    {SKILL_MARKET_OPTIONS.map((opt) => (
                      <SelectItem key={opt.value} value={opt.value}>
                        {opt.label}
                      </SelectItem>
                    ))}
                  </SelectContent>
                </Select>
              </Row>
            </CardContent>
            <CardFooter>
              <Button
                disabled={saving}
                onClick={() => {
                  setTheme(settings.theme);
                  void (async () => {
                    setSaving(true);
                    try {
                      const next = await updateSettings({
                        language: settings.language,
                        theme: settings.theme,
                        autoStart: settings.autoStart,
                        closeToTray: settings.closeToTray,
                        skillMarketSource: settings.skillMarketSource ?? 'auto',
                      });
                      setSettings(next);
                      // 必须在保存成功后再清市场缓存，否则会继续展示上一源
                      invalidateSkills('market');
                      toast({
                        title: '常规设置已保存',
                        description: generalSettingsSaveDescription(next),
                        variant: 'success',
                      });
                    } catch (e) {
                      toast({
                        title: '保存失败',
                        description: String(e),
                        variant: 'danger',
                      });
                    } finally {
                      setSaving(false);
                    }
                  })();
                }}
              >
                {saving ? '保存中…' : '保存'}
              </Button>
            </CardFooter>
          </Card>
        </TabsContent>

        {/* 安全：只读说明；不提供主密码 / 落盘加密配置入口（AGENTS.md） */}
        <TabsContent value="security">
          <Card>
            <CardContent className="divide-y divide-border pt-1">
              <Row
                label="凭据展示"
                description="界面不回显明文"
                descriptionTip="密钥与令牌经 SecretInput 管理；默认脱敏显示，可点眼睛切换明文。"
              >
                <Badge variant="success">不回显</Badge>
              </Row>
              <Row
                label="存储方式"
                description="系统自动选择"
                descriptionTip="只读展示；当前无需配置主密码或落盘加密。"
              >
                <span className="text-sm text-secondary">
                  {isKeyring ? '系统 keyring' : '本地存储'}
                </span>
                <Badge variant={isKeyring ? 'success' : 'default'}>
                  {isKeyring ? 'keyring' : 'local'}
                </Badge>
              </Row>
            </CardContent>
          </Card>
        </TabsContent>

        {/* 数据 */}
        <TabsContent value="data">
          <Card>
            <CardContent className="divide-y divide-border pt-1">
              <Row
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
              </Row>
              <Row
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
              </Row>
              <Row
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
              </Row>
              <Row
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
              </Row>
              <Row
                label="用量采集间隔"
                description="分钟；0=仅手动"
                descriptionTip="App 前台时按间隔自动增量采集，后台暂停。总览显示上次/下次同步。"
              >
                <Input
                  type="number"
                  min={0}
                  className="w-20"
                  value={settings.usageCollectIntervalMin}
                  onChange={(e) => {
                    const n = parseInt(e.target.value, 10);
                    patch({ usageCollectIntervalMin: Number.isNaN(n) ? 0 : Math.max(0, n) });
                  }}
                />
                <Link
                  to="/?section=usage"
                  className="inline-flex h-7 items-center justify-center rounded-btn border border-border bg-transparent px-2.5 text-xs font-medium text-secondary transition-colors hover:bg-hover hover:text-primary"
                >
                  查看
                </Link>
              </Row>
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
        </TabsContent>

        {/* 备份：本机配置快照；操作即时生效，无底部保存 */}
        <TabsContent value="backups">
          <BackupsPanel />
        </TabsContent>

        {/* 关于 */}
        <TabsContent value="about">
          <Card>
            <CardContent className="divide-y divide-border pt-1">
              <Row
                label="版本"
                description={
                  pendingUpdate
                    ? `可更新至 v${pendingUpdate.version}`
                    : '检查发布频道的新版本'
                }
              >
                <span className="flex items-center gap-2 font-mono text-sm text-secondary">
                  v{settings.appVersion}
                  {pendingUpdate && <Badge variant="warning">有新版本</Badge>}
                </span>
                <Button
                  size="sm"
                  variant="outline"
                  disabled={checking || installing}
                  onClick={checkUpdate}
                >
                  {checking ? '检查中…' : '检查更新'}
                </Button>
              </Row>
              {pendingUpdate && (
                <Row
                  label="新版本"
                  description={pendingUpdate.notes?.split('\n')[0] || '签名安装包一键升级'}
                  descriptionTip={pendingUpdate.notes || undefined}
                >
                  <span className="font-mono text-sm text-accent">
                    v{pendingUpdate.version}
                  </span>
                  <Button size="sm" disabled={installing || checking} onClick={installUpdate}>
                    {installing ? '更新中…' : '一键更新'}
                  </Button>
                </Row>
              )}
              <Row
                label="GitHub"
                description="源码、Issue 与发布页"
                descriptionTip={GITHUB_REPO_URL}
              >
                <Button
                  size="sm"
                  variant="outline"
                  onClick={() => {
                    void openExternalLink(GITHUB_REPO_URL).catch((e) => {
                      toast({
                        title: '无法打开 GitHub',
                        description: e instanceof Error ? e.message : String(e),
                        variant: 'danger',
                      });
                    });
                  }}
                >
                  <ExternalLink className="mr-1.5 h-3.5 w-3.5" />
                  打开仓库
                </Button>
              </Row>
            </CardContent>
          </Card>
          <p className="mt-3 text-xs text-muted">
            AgentHub — 统一管理 AI coding agent 的配置、账号与用量。
          </p>
        </TabsContent>
      </Tabs>
    </div>
  );
}
