import { Card, CardContent, CardFooter } from '@/components/ui/card';
import { Button } from '@/components/ui/button';
import { Switch } from '@/components/ui/switch';
import {
  Select,
  SelectValue,
  SelectTrigger,
  SelectContent,
  SelectItem,
} from '@/components/ui/select';
import { useToast } from '@/components/ui/toast';
import { useI18n } from '@/components/shared/LanguageProvider';
import { updateSettings } from '@/lib/api/settings';
import { invalidateSkills } from '@/lib/hooks/useSkills';
import type { AppSettings } from '@/lib/types';
import { applyTheme } from '@/lib/theme';
import { useTheme } from '@/components/shared/ThemeProvider';
import {
  generalSettingsPayload,
  generalSettingsSaveDescription,
  SKILL_MARKET_VALUES,
  skillMarketLabel,
} from './settings-format';
import { SettingsRow } from './settings-shared';

export function GeneralPanel({
  settings,
  patch,
  setSettings,
  committedThemeRef,
  committedLanguageRef,
  saving,
  setSaving,
}: {
  settings: AppSettings;
  patch: (p: Partial<AppSettings>) => void;
  setSettings: (s: AppSettings) => void;
  committedThemeRef: React.MutableRefObject<AppSettings['theme']>;
  committedLanguageRef: React.MutableRefObject<AppSettings['language']>;
  saving: boolean;
  setSaving: (v: boolean) => void;
}) {
  const { toast } = useToast();
  const { setTheme } = useTheme();
  const { t, setLanguage } = useI18n();
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
                  onCheckedChange={(v) => patch({ autoStart: v })}
                />
              </SettingsRow>
              <SettingsRow
                label={t('settings.general.closeToTrayLabel')}
                description={t('settings.general.closeToTrayDescription')}
                descriptionTip={t('settings.general.closeToTrayTip')}
              >
                <Switch
                  checked={settings.closeToTray}
                  onCheckedChange={(v) => patch({ closeToTray: v })}
                />
              </SettingsRow>
              <SettingsRow
                label={t('settings.general.skillMarketLabel')}
                description={t('settings.general.skillMarketDescription')}
                descriptionTip={t('settings.general.skillMarketTip')}
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
                    {SKILL_MARKET_VALUES.map((value) => (
                      <SelectItem key={value} value={value}>
                        {skillMarketLabel(value, t)}
                      </SelectItem>
                    ))}
                  </SelectContent>
                </Select>
              </SettingsRow>
            </CardContent>
            <CardFooter>
              <Button
                disabled={saving}
                onClick={() => {
                  void (async () => {
                    setSaving(true);
                    try {
                      const next = await updateSettings(generalSettingsPayload(settings));
                      setSettings(next);
                      committedThemeRef.current = next.theme;
                      committedLanguageRef.current = next.language;
                      setTheme(next.theme);
                      setLanguage(next.language);
                      // 必须在保存成功后再清市场缓存，否则会继续展示上一源
                      invalidateSkills('market');
                      toast({
                        title: t('settings.general.savedToast'),
                        description: generalSettingsSaveDescription(next, t),
                        variant: 'success',
                      });
                    } catch (e) {
                      toast({
                        title: t('common.saveFailed'),
                        description: String(e),
                        variant: 'danger',
                      });
                    } finally {
                      setSaving(false);
                    }
                  })();
                }}
              >
                {saving ? t('common.saving') : t('common.save')}
              </Button>
            </CardFooter>
          </Card>
  );
}
