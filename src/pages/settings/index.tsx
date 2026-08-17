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
import { useI18n, syncLanguageFromSettings } from '@/components/shared/LanguageProvider';
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
  const { t, setLanguage } = useI18n();
  const { theme: providerTheme, setTheme } = useTheme();
  const committedThemeRef = useRef(providerTheme);
  const committedLanguageRef = useRef(lang);
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
      committedLanguageRef.current = s.language;
      setTheme(s.theme);
      setLanguage(s.language);
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
      syncLanguageFromSettings({ language: committedLanguageRef.current });
      setLanguage(committedLanguageRef.current);
    };
  }, [setLanguage]);

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
            toast({ title: t('settings.page.latestVersion'), variant: 'success' });
          }
          return;
        }
        if (!(await isUpdateAvailable())) {
          toast({
            title: t('settings.page.cannotCheckUpdate'),
            description: t('settings.page.desktopOnlyUpdate'),
            variant: 'danger',
          });
          return;
        }
        const found = await checkForUpdate();
        setAppUpdateAvailable(found);
        if (!found) {
          toast({ title: t('settings.page.latestVersion'), variant: 'success' });
        } else {
          toast({
            title: t('settings.page.updateFound', { version: found.version }),
            description: t('settings.page.updateFoundDesc'),
            variant: 'success',
          });
        }
      } catch (e) {
        toast({
          title: t('settings.page.checkUpdateFailed'),
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
          title: t('settings.page.updateInstalled'),
          description: t('settings.page.updateInstalledDesc'),
          variant: 'success',
        });
      } catch (e) {
        toast({
          title: t('settings.page.updateFailed'),
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
          title={t('settings.page.title')}
          description={t('settings.page.description')}
          descriptionTip={t('settings.page.descriptionTip')}
        />
        <SettingsSkeleton />
      </div>
    );
  }

  if (error || !settings) {
    return (
      <div>
        <PageHeader
          title={t('settings.page.title')}
          description={t('settings.page.description')}
          descriptionTip={t('settings.page.descriptionTip')}
        />
        <ErrorState error={error ?? new Error(t('settings.page.emptyError'))} onRetry={() => void load()} />
      </div>
    );
  }

  return (
    <div>
      <PageHeader
        title={t('settings.page.title')}
        description={t('settings.page.description')}
        descriptionTip={t('settings.page.descriptionTip')}
      />

      <Tabs value={tab} onValueChange={setTab}>
        <div className={pageRhythm.chrome}>
          <TabsList>
            <TabsTrigger value="general">{t('settings.page.tabGeneral')}</TabsTrigger>
            <TabsTrigger value="security">{t('settings.page.tabSecurity')}</TabsTrigger>
            <TabsTrigger value="data">{t('settings.page.tabData')}</TabsTrigger>
            <TabsTrigger value="backups">{t('settings.page.tabBackups')}</TabsTrigger>
            <TabsTrigger value="about" className="gap-1.5">
              {t('settings.page.tabAbout')}
              {pendingUpdate && (
                <StatusPin
                  tone="warning"
                  label={t('settings.page.updatePin', { version: pendingUpdate.version })}
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
            committedLanguageRef={committedLanguageRef}
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
