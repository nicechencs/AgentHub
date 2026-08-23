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
import {
  createSettingsPersistenceTracker,
  mergeSettingsResponse,
  persistSettingsPatch,
} from './settings-persist';
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
  setSettings: React.Dispatch<React.SetStateAction<AppSettings | null>>;
  committedThemeRef: React.MutableRefObject<AppSettings['theme']>;
  committedLanguageRef: React.MutableRefObject<AppSettings['language']>;
}) {
  const { toast } = useToast();
  const { setTheme } = useTheme();
  const { t, setLanguage } = useI18n();
  const usageBaselineRef = useRef(settings.usageCollectIntervalMin);
  const persistenceTrackerRef = useRef<ReturnType<typeof createSettingsPersistenceTracker> | null>(null);

  const persist = async (
    nextPatch: Partial<AppSettings>,
    after?: (saved: AppSettings) => void,
  ) => {
    const tracker =
      persistenceTrackerRef.current ??
      (persistenceTrackerRef.current = createSettingsPersistenceTracker());
    const generation = tracker.begin(nextPatch, {
      ...settings,
      theme: committedThemeRef.current,
      language: committedLanguageRef.current,
    });
    const requestedKeys = Object.keys(nextPatch) as Array<keyof AppSettings>;
    try {
      // updateSettings splits durable write from snapshot refresh. A resolved
      // value is already committed; only a thrown write error rolls the UI back.
      const saved = await persistSettingsPatch(nextPatch);
      const settlement = tracker.settleSuccess(generation, saved, requestedKeys);
      if (settlement.committedPatch.theme !== undefined) {
        committedThemeRef.current = settlement.committedPatch.theme;
      }
      if (settlement.committedPatch.language !== undefined) {
        committedLanguageRef.current = settlement.committedPatch.language;
      }
      if (settlement.ownedKeys.length === 0) return;
      const ownedPatch = Object.fromEntries(
        settlement.ownedKeys.map((key) => [key, saved[key]]),
      ) as Partial<AppSettings>;
      setSettings((current) =>
        current ? mergeSettingsResponse(current, saved, ownedPatch) : current,
      );
      if (settlement.ownedKeys.length === requestedKeys.length) after?.(saved);
    } catch (e) {
      const settlement = tracker.settleFailure(generation);
      if (settlement.ownedKeys.length === 0) return;
      setSettings((current) =>
        current ? { ...current, ...settlement.rollbackPatch } : current,
      );
      if (settlement.rollbackPatch.theme !== undefined) {
        const theme = settlement.rollbackPatch.theme;
        committedThemeRef.current = theme;
        applyTheme(theme);
        setTheme(theme);
      }
      if (settlement.rollbackPatch.language !== undefined) {
        const language = settlement.rollbackPatch.language;
        committedLanguageRef.current = language;
        setLanguage(language);
      }
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
              patch({ language });
              setLanguage(language);
              void persist({ language });
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
              patch({ theme });
              applyTheme(theme);
              void persist({ theme }, (saved) => {
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
              patch({ autoStart: v });
              void persist({ autoStart: v });
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
              patch({ closeToTray: v });
              void persist({ closeToTray: v });
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
              patch({ skillMarketSource });
              void persist({ skillMarketSource }, () => {
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
