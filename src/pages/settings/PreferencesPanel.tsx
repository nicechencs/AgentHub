import { useRef } from 'react';
import { Card, CardContent } from '@/components/ui/card';
import { Switch } from '@/components/ui/switch';
import {
  Select,
  SelectValue,
  SelectTrigger,
  SelectContent,
  SelectItem,
} from '@/components/ui/select';
import { Input } from '@/components/ui/input';
import { Link } from 'react-router-dom';
import { useToast } from '@/components/ui/toast';
import { useI18n } from '@/components/shared/LanguageProvider';
import { useTheme } from '@/components/shared/ThemeProvider';
import { invalidateSkills } from '@/lib/hooks/useSkills';
import type { AppSettings } from '@/lib/types';
import { applyTheme } from '@/lib/theme';
import { notifyUsageSettingsChanged } from '@/lib/usage-sync';
import { persistSettingsPatch } from './settings-persist';
import {
  SKILL_MARKET_VALUES,
  clampUsageIntervalMin,
  skillMarketLabel,
} from './settings-format';
import { SettingsRow } from './settings-shared';

export function PreferencesPanel({
  settings,
  patch,
  setSettings,
  committedThemeRef,
  committedLanguageRef,
}: {
  settings: AppSettings;
  patch: (p: Partial<AppSettings>) => void;
  setSettings: (s: AppSettings) => void;
  committedThemeRef: React.MutableRefObject<AppSettings['theme']>;
  committedLanguageRef: React.MutableRefObject<AppSettings['language']>;
}) {
  const { toast } = useToast();
  const { setTheme } = useTheme();
  const { t, setLanguage } = useI18n();
  const usageBaselineRef = useRef(settings.usageCollectIntervalMin);

  const persist = async (
    nextPatch: Partial<AppSettings>,
    revert: () => void,
    after?: (saved: AppSettings) => void,
  ) => {
    try {
      const saved = await persistSettingsPatch(nextPatch);
      setSettings(saved);
      committedThemeRef.current = saved.theme;
      committedLanguageRef.current = saved.language;
      after?.(saved);
    } catch (e) {
      revert();
      toast({
        title: t('common.saveFailed'),
        description: String(e),
        variant: 'danger',
      });
    }
  };

  return (
    <Card>
      <CardContent className="divide-y divide-border pt-1">
        <SettingsRow
          label={t('settings.general.languageLabel')}
          description={t('settings.general.languageDescription')}
        >
          <Select
            value={settings.language}
            onValueChange={(v) => {
              const language = v as AppSettings['language'];
              const prev = settings.language;
              patch({ language });
              setLanguage(language);
              void persist({ language }, () => {
                patch({ language: prev });
                setLanguage(prev);
              });
            }}
          >
            <SelectTrigger className="w-full">
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              <SelectItem value="zh">{t('settings.general.languageZh')}</SelectItem>
              <SelectItem value="en">{t('settings.general.languageEn')}</SelectItem>
            </SelectContent>
          </Select>
        </SettingsRow>
        <SettingsRow
          label={t('settings.general.themeLabel')}
          description={t('settings.general.themeDescription')}
        >
          <Select
            value={settings.theme}
            onValueChange={(v) => {
              const theme = v as AppSettings['theme'];
              const prev = settings.theme;
              patch({ theme });
              applyTheme(theme);
              void persist({ theme }, () => {
                patch({ theme: prev });
                applyTheme(prev);
              }, (saved) => {
                setTheme(saved.theme);
              });
            }}
          >
            <SelectTrigger className="w-full">
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              <SelectItem value="light">{t('settings.general.themeLight')}</SelectItem>
              <SelectItem value="dark">{t('settings.general.themeDark')}</SelectItem>
              <SelectItem value="system">{t('settings.general.themeSystem')}</SelectItem>
            </SelectContent>
          </Select>
        </SettingsRow>
        <SettingsRow
          label={t('settings.general.autoStartLabel')}
          description={t('settings.general.autoStartDescription')}
          descriptionTip={t('settings.general.autoStartTip')}
        >
          <Switch
            checked={settings.autoStart}
            onCheckedChange={(v) => {
              const prev = settings.autoStart;
              patch({ autoStart: v });
              void persist({ autoStart: v }, () => patch({ autoStart: prev }));
            }}
          />
        </SettingsRow>
        <SettingsRow
          label={t('settings.general.closeToTrayLabel')}
          description={t('settings.general.closeToTrayDescription')}
          descriptionTip={t('settings.general.closeToTrayTip')}
        >
          <Switch
            checked={settings.closeToTray}
            onCheckedChange={(v) => {
              const prev = settings.closeToTray;
              patch({ closeToTray: v });
              void persist({ closeToTray: v }, () => patch({ closeToTray: prev }));
            }}
          />
        </SettingsRow>
        <SettingsRow
          label={t('settings.general.skillMarketLabel')}
          description={t('settings.general.skillMarketDescription')}
          descriptionTip={t('settings.general.skillMarketTip')}
        >
          <Select
            value={settings.skillMarketSource ?? 'auto'}
            onValueChange={(v) => {
              const skillMarketSource = v as AppSettings['skillMarketSource'];
              const prev = settings.skillMarketSource;
              patch({ skillMarketSource });
              void persist({ skillMarketSource }, () => patch({ skillMarketSource: prev }), () => {
                invalidateSkills('market');
              });
            }}
          >
            <SelectTrigger className="w-full">
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              {SKILL_MARKET_VALUES.map((value) => (
                <SelectItem key={value} value={value}>
                  {skillMarketLabel(value, t)}
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
        </SettingsRow>
        <SettingsRow
          label={t('settings.data.usageIntervalLabel')}
          description={t('settings.data.usageIntervalDescription')}
          descriptionTip={t('settings.data.usageIntervalTip')}
        >
          <Input
            type="number"
            min={0}
            max={24 * 60}
            className="w-20"
            value={settings.usageCollectIntervalMin}
            onFocus={() => {
              usageBaselineRef.current = settings.usageCollectIntervalMin;
            }}
            onChange={(e) => {
              const n = parseInt(e.target.value, 10);
              if (Number.isNaN(n)) {
                patch({ usageCollectIntervalMin: 0 });
                return;
              }
              patch({ usageCollectIntervalMin: clampUsageIntervalMin(n) });
            }}
            onBlur={() => {
              const value = clampUsageIntervalMin(settings.usageCollectIntervalMin);
              if (value === usageBaselineRef.current) return;
              void persist(
                { usageCollectIntervalMin: value },
                () => patch({ usageCollectIntervalMin: usageBaselineRef.current }),
                () => {
                  notifyUsageSettingsChanged();
                },
              );
            }}
          />
          <Link
            to="/?section=usage"
            className="inline-flex h-7 items-center justify-center rounded-btn border border-border bg-transparent px-2.5 text-xs font-medium text-secondary transition-colors hover:bg-hover hover:text-primary"
          >
            {t('common.view')}
          </Link>
        </SettingsRow>
      </CardContent>
    </Card>
  );
}
