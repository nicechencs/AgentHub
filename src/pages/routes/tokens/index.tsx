import { KeyRound } from 'lucide-react';
import { PageHeader } from '@/components/layout/PageHeader';
import { useI18n } from '@/components/shared/LanguageProvider';
import { RoutesPane } from '@/pages/routes/RoutesPane';
import { SoonPane } from '@/pages/routes/SoonPane';

export default function RoutesTokensPage() {
  const { t } = useI18n();
  return (
    <RoutesPane>
      <PageHeader
        title={t('routes.tokens.title')}
        description={t('routes.tokens.description')}
      />
      <SoonPane
        icon={KeyRound}
        title={t('routes.tokens.soonTitle')}
        description={t('routes.tokens.soonDescription')}
      />
    </RoutesPane>
  );
}
