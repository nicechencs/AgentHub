import { Badge } from '@/components/ui/badge';
import { Card, CardContent } from '@/components/ui/card';
import { useI18n } from '@/components/shared/LanguageProvider';
import { SettingsRow } from './settings-shared';

export function SecurityPanel() {
  const { t } = useI18n();
  return (
          <Card>
            <CardContent className="divide-y divide-border pt-1">
              <SettingsRow
                label={t('settings.security.displayLabel')}
                description={t('settings.security.displayDescription')}
                descriptionTip={t('settings.security.displayTip')}
              >
                <Badge variant="success">{t('settings.security.displayBadge')}</Badge>
              </SettingsRow>
              <SettingsRow
                label={t('settings.security.storeLabel')}
                description={t('settings.security.storeDescription')}
                descriptionTip={t('settings.security.storeTip')}
              >
                <span className="text-sm text-secondary">{t('settings.security.storeValue')}</span>
              </SettingsRow>
            </CardContent>
          </Card>
  );
}
