import { useRef } from 'react';
import { useI18n } from '@/components/shared/LanguageProvider';
import { OpenDirButton } from '@/components/shared/OpenDirButton';
import { Card, CardContent } from '@/components/ui/card';
import { Input } from '@/components/ui/input';
import { Tip } from '@/components/ui/tooltip';
import {
  Select,
  SelectValue,
  SelectTrigger,
  SelectContent,
  SelectItem,
} from '@/components/ui/select';
import { useToast } from '@/components/ui/toast';
import { openLogsDir } from '@/lib/api/settings';
import { openPathInFileManager } from '@/lib/api/skill';
import type { AppSettings, LogLevel } from '@/lib/types';
import { persistSettingsPatch } from './settings-persist';
import {
  LOG_LEVEL_VALUES,
  clampLogRetentionDays,
  logLevelOptionLabel,
} from './settings-format';
import { SettingsRow } from './settings-shared';

export function LocalPanel({
  settings,
  patch,
  setSettings,
}: {
  settings: AppSettings;
  patch: (p: Partial<AppSettings>) => void;
  setSettings: (s: AppSettings) => void;
}) {
  const { toast } = useToast();
  const { t } = useI18n();
  const retentionBaselineRef = useRef(settings.logRetentionDays);

  const persist = async (
    nextPatch: Partial<AppSettings>,
    revert: () => void,
    after?: (saved: AppSettings) => void,
  ) => {
    try {
      const saved = await persistSettingsPatch(nextPatch);
      setSettings(saved);
      after?.(saved);
    } catch (e) {
      revert();
      toast({ title: t('common.saveFailed'), description: String(e), variant: 'danger' });
    }
  };

  return (
    <Card>
      <CardContent className="divide-y divide-border pt-1">
          <SettingsRow
            wide
            label={t('settings.data.dataDirLabel')}
            description={t('settings.data.dataDirDescription')}
            descriptionTip={t('settings.data.dataDirTip')}
          >
            <Tip
              className="min-w-0 flex-1 break-all text-right font-mono text-xs text-secondary"
              label={settings.dataDir}
            >
              {settings.dataDir}
            </Tip>
            <OpenDirButton
              labeled
              title={settings.dataDir}
              onClick={() => {
                void (async () => {
                  try {
                    const p = await openPathInFileManager(settings.dataDir);
                    toast({ title: t('settings.data.dataDirOpened'), description: p, variant: 'success' });
                  } catch (e) {
                    toast({
                      title: t('settings.data.dataDirOpenFailed'),
                      description: String(e),
                      variant: 'danger',
                    });
                  }
                })();
              }}
            />
          </SettingsRow>
          <SettingsRow
            label={t('settings.data.logLevelLabel')}
            description={t('settings.data.logLevelDescription')}
            descriptionTip={t('settings.data.logLevelTip')}
          >
            <Select
              value={settings.logLevel}
              onValueChange={(v) => {
                const logLevel = v as LogLevel;
                const prev = settings.logLevel;
                patch({ logLevel });
                void persist({ logLevel }, () => patch({ logLevel: prev }), (saved) => {
                  toast({
                    title: t('settings.data.logLevelSavedToast'),
                    description: t('settings.data.logLevelRestart', {
                      level: saved.logLevel,
                      days: saved.logRetentionDays,
                    }),
                    variant: 'success',
                  });
                });
              }}
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
              onFocus={() => {
                retentionBaselineRef.current = settings.logRetentionDays;
              }}
              onChange={(e) => {
                const n = parseInt(e.target.value, 10);
                if (Number.isNaN(n)) {
                  patch({ logRetentionDays: 14 });
                  return;
                }
                patch({ logRetentionDays: clampLogRetentionDays(n) });
              }}
              onBlur={() => {
                const value = clampLogRetentionDays(settings.logRetentionDays);
                if (value === retentionBaselineRef.current) return;
                void persist(
                  { logRetentionDays: value },
                  () => patch({ logRetentionDays: retentionBaselineRef.current }),
                );
              }}
            />
          </SettingsRow>
          <SettingsRow
            wide
            label={t('settings.data.logsDirLabel')}
            description={t('settings.data.logsDirDescription')}
            descriptionTip={t('settings.data.logsDirTip')}
          >
            <Tip
              className="min-w-0 flex-1 break-all text-right font-mono text-xs text-secondary"
              label={settings.logsDir}
            >
              {settings.logsDir}
            </Tip>
            <OpenDirButton
              labeled
              title={settings.logsDir}
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
            />
          </SettingsRow>
      </CardContent>
    </Card>
  );
}
