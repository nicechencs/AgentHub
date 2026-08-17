import { ExternalLink } from 'lucide-react';
import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import { Card, CardContent } from '@/components/ui/card';
import { useToast } from '@/components/ui/toast';
import { useI18n } from '@/components/shared/LanguageProvider';
import type { UpdateInfo } from '@/lib/api/update';
import { openExternalLink } from '@/lib/open-external';
import type { AppSettings } from '@/lib/types';
import { SecurityPanel } from './SecurityPanel';
import { GITHUB_REPO_URL } from './settings-format';
import { SettingsRow } from './settings-shared';

export function AboutPanel({
  settings,
  pendingUpdate,
  checking,
  installing,
  checkUpdate,
  installUpdate,
}: {
  settings: AppSettings;
  pendingUpdate: UpdateInfo | null;
  checking: boolean;
  installing: boolean;
  checkUpdate: () => void;
  installUpdate: () => void;
}) {
  const { toast } = useToast();
  const { t } = useI18n();
  return (
    <>
      <Card>
        <CardContent className="divide-y divide-border pt-1">
          <SettingsRow
            label={t('settings.about.versionLabel')}
            description={
              pendingUpdate
                ? t('settings.about.versionUpdate', { version: pendingUpdate.version })
                : t('settings.about.versionDescription')
            }
          >
            <span className="flex items-center gap-2 font-mono text-sm text-secondary">
              v{settings.appVersion}
              {pendingUpdate && <Badge variant="warning">{t('settings.about.newVersionBadge')}</Badge>}
            </span>
            <Button
              size="sm"
              variant="outline"
              disabled={checking || installing}
              onClick={checkUpdate}
            >
              {checking ? t('settings.about.checking') : t('settings.about.checkUpdate')}
            </Button>
          </SettingsRow>
          {pendingUpdate && (
            <SettingsRow
              label={t('settings.about.newVersionLabel')}
              description={pendingUpdate.notes?.split('\n')[0] || t('settings.about.newVersionFallback')}
              descriptionTip={pendingUpdate.notes || undefined}
            >
              <span className="font-mono text-sm text-accent">
                v{pendingUpdate.version}
              </span>
              <Button size="sm" disabled={installing || checking} onClick={installUpdate}>
                {installing ? t('settings.about.updating') : t('settings.about.oneClickUpdate')}
              </Button>
            </SettingsRow>
          )}
          <SettingsRow
            label={t('settings.about.githubLabel')}
            description={t('settings.about.githubDescription')}
            descriptionTip={GITHUB_REPO_URL}
          >
            <Button
              size="sm"
              variant="outline"
              onClick={() => {
                void openExternalLink(GITHUB_REPO_URL).catch((e) => {
                  toast({
                    title: t('settings.about.openGithubFailed'),
                    description: e instanceof Error ? e.message : String(e),
                    variant: 'danger',
                  });
                });
              }}
            >
              <ExternalLink className="mr-1.5 h-3.5 w-3.5" />
              {t('settings.about.openRepo')}
            </Button>
          </SettingsRow>
        </CardContent>
      </Card>
      <div className="mt-4">
        <SecurityPanel />
      </div>
      <p className="mt-3 text-xs text-muted">
        {t('settings.about.tagline')}
      </p>
    </>
  );
}
