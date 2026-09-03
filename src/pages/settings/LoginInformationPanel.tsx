import { Badge } from '@/components/ui/badge';
import { Card, CardContent } from '@/components/ui/card';
import { useI18n } from '@/components/shared/LanguageProvider';
import { SettingsRow } from './settings-shared';

/** About 页：API Key 与登录信息的展示 / 保存说明（非完整安全设置）。 */
export function LoginInformationPanel() {
  const { t } = useI18n();
  return (
          <Card>
            <CardContent className="divide-y divide-border pt-1">
              <SettingsRow
                label={t('settings.loginInfo.displayLabel')}
                description={t('settings.loginInfo.displayDescription')}
                descriptionTip={t('settings.loginInfo.displayTip')}
              >
                <Badge variant="success">{t('settings.loginInfo.displayBadge')}</Badge>
              </SettingsRow>
              <SettingsRow
                label={t('settings.loginInfo.storeLabel')}
                description={t('settings.loginInfo.storeDescription')}
                descriptionTip={t('settings.loginInfo.storeTip')}
              >
                <span className="text-sm text-secondary">{t('settings.loginInfo.storeValue')}</span>
              </SettingsRow>
            </CardContent>
          </Card>
  );
}
