import { PageHeader } from '@/components/layout/PageHeader';
import { useI18n } from '@/components/shared/LanguageProvider';
import { BackupsPanel } from './BackupsPanel';

export default function BackupsPage() {
  const { t } = useI18n();
  return (
    <div>
      <PageHeader
        title={t('settings.backups.pageTitle')}
        description={t('settings.backups.pageDescription')}
        descriptionTip={t('settings.backups.pageDescriptionTip')}
      />
      <BackupsPanel />
    </div>
  );
}
