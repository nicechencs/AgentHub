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
import { useI18n } from '@/components/shared/LanguageProvider';
import { openLogsDir, updateSettings } from '@/lib/api/settings';
import { BRIDGES_PATH } from '@/lib/bridges-path';
import type { AppSettings, LogLevel } from '@/lib/types';
import { notifyUsageSettingsChanged } from '@/lib/usage-sync';
import {
  dataSettingsSaveDescription,
  LOG_LEVEL_VALUES,
  logLevelOptionLabel,
} from './settings-format';
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
  const { t } = useI18n();
  return (
          <Card>
            <CardContent className="divide-y divide-border pt-1">
              <SettingsRow
                label={t('settings.data.routesLabel')}
                description={t('settings.data.routesDescription')}
                descriptionTip={t('settings.data.routesTip')}
              >
                <Link
                  to={BRIDGES_PATH}
                  className="inline-flex h-7 items-center justify-center rounded-btn border border-border bg-transparent px-2.5 text-xs font-medium text-secondary transition-colors hover:bg-hover hover:text-primary"
                >
                  {t('common.open')}
                </Link>
              </SettingsRow>
              <SettingsRow
                label={t('settings.data.dataDirLabel')}
                description={t('settings.data.dataDirDescription')}
                descriptionTip={t('settings.data.dataDirTip')}
              >
                <Input
                  className="w-full font-mono text-xs"
                  value={settings.dataDir}
                  readOnly
                  title={settings.dataDir}
                />
              </SettingsRow>
              <SettingsRow
                label={t('settings.data.logLevelLabel')}
                description={t('settings.data.logLevelDescription')}
                descriptionTip={t('settings.data.logLevelTip')}
              >
                <Select
                  value={settings.logLevel}
                  onValueChange={(v) => patch({ logLevel: v as LogLevel })}
                >
                  <SelectTrigger className="w-full">
                    <SelectValue />
                  </SelectTrigger>
                  <SelectContent>
                    {LOG_LEVEL_VALUES.map((value) => (
                      <SelectItem key={value} value={value}>
                        {logLevelOptionLabel(value, t)}
                      </SelectItem>
                    ))}
                  </SelectContent>
                </Select>
              </SettingsRow>
              <SettingsRow
                label={t('settings.data.logRetentionLabel')}
                description={t('settings.data.logRetentionDescription')}
                descriptionTip={t('settings.data.logRetentionTip')}
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
                label={t('settings.data.logsDirLabel')}
                description={t('settings.data.logsDirDescription')}
                descriptionTip={t('settings.data.logsDirTip')}
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
                        toast({ title: t('settings.data.logsOpened'), description: p, variant: 'success' });
                      } catch (e) {
                        toast({
                          title: t('settings.data.logsOpenFailed'),
                          description: String(e),
                          variant: 'danger',
                        });
                      }
                    })();
                  }}
                >
                  {t('common.open')}
                </Button>
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
                  {t('common.view')}
                </Link>
              </SettingsRow>
            </CardContent>
            <CardFooter className="flex flex-col items-stretch gap-2 sm:flex-row sm:items-center sm:justify-between">
              <p className="text-xs text-muted">
                {t('settings.data.footerNote')}
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
                        title: t('settings.data.savedToast'),
                        description: dataSettingsSaveDescription(next, t),
                        variant: 'success',
                      });
                    } catch (e) {
                      toast({ title: t('common.saveFailed'), description: String(e), variant: 'danger' });
                    } finally {
                      setSaving(false);
                    }
                  })()
                }
              >
                {saving ? t('common.saving') : t('common.save')}
              </Button>
            </CardFooter>
          </Card>
  );
}
