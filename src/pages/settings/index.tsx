// Settings 设置页(docs/ui-design.md §4.8)
// Tabs:常规 / 安全 / 数据 / 备份 / 关于；tab 与 ?tab= URL 同步。
// 常规/数据草稿态编辑后点 [保存]；备份分区操作即时生效，无底部保存。
import { useEffect, useRef, useState } from 'react';
import { useSearchParams } from 'react-router-dom';
import { PageHeader } from '@/components/layout/PageHeader';
import { pageRhythm } from '@/components/layout/page-rhythm';
import { Tabs, TabsList, TabsTrigger, TabsContent } from '@/components/ui/tabs';
import { useToast } from '@/components/ui/toast';
import { ErrorState } from '@/components/shared/ErrorState';
import { StatusPin } from '@/components/shared/StatusPin';
import { useTheme } from '@/components/shared/ThemeProvider';
import {
  getSettings,
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
import { applyTheme } from '@/lib/theme';
import type { AppSettings } from '@/lib/types';
import { BackupsPanel } from './BackupsPanel';
import { AboutPanel } from './AboutPanel';
import { DataPanel } from './DataPanel';
import { GeneralPanel } from './GeneralPanel';
import { SecurityPanel } from './SecurityPanel';
import { parseSettingsTab } from './settings-format';
import { SettingsSkeleton } from './settings-shared';

export default function SettingsPage({
  onCheckUpdate,
}: {
  /** Optional shared check from App UpdatePrompt (opens global dialog when found). */
  onCheckUpdate?: () => Promise<UpdateInfo | null>;
} = {}) {
  const { toast } = useToast();
  const { theme: providerTheme, setTheme } = useTheme();
  const committedThemeRef = useRef(providerTheme);
  const [searchParams, setSearchParams] = useSearchParams();
  const tab = parseSettingsTab(searchParams.get('tab'));

  const [settings, setSettings] = useState<AppSettings | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<unknown>(null);
  const [saving, setSaving] = useState(false);

  const [checking, setChecking] = useState(false);
  const [installing, setInstalling] = useState(false);
  const pendingUpdate = useAppUpdateAvailable();

  const load = async () => {
    setLoading(true);
    setError(null);
    try {
      const s = await getSettings();
      try {
        const v = await getAppVersion();
        if (v) s.appVersion = v;
      } catch {
        // keep settings.appVersion fallback
      }
      setSettings(s);
      committedThemeRef.current = s.theme;
      setTheme(s.theme);
    } catch (e) {
      setError(e);
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    void load();
  }, []);

  useEffect(() => {
    return () => {
      applyTheme(committedThemeRef.current);
    };
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

  return (
    <div>
      <PageHeader
        title="设置"
        description="偏好与数据"
        descriptionTip="主题、安全展示、日志、用量采集间隔与备份。"
      />

      <Tabs value={tab} onValueChange={setTab}>
        <div className={pageRhythm.chrome}>
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
        </div>

        <TabsContent value="general">
          <GeneralPanel
            settings={settings}
            patch={patch}
            setSettings={setSettings}
            committedThemeRef={committedThemeRef}
            saving={saving}
            setSaving={setSaving}
          />
        </TabsContent>

        <TabsContent value="security">
          <SecurityPanel />
        </TabsContent>

        <TabsContent value="data">
          <DataPanel
            settings={settings}
            patch={patch}
            setSettings={setSettings}
            saving={saving}
            setSaving={setSaving}
          />
        </TabsContent>

        <TabsContent value="backups">
          <BackupsPanel />
        </TabsContent>

        <TabsContent value="about">
          <AboutPanel
            settings={settings}
            pendingUpdate={pendingUpdate}
            checking={checking}
            installing={installing}
            checkUpdate={checkUpdate}
            installUpdate={installUpdate}
          />
        </TabsContent>
      </Tabs>
    </div>
  );
}
