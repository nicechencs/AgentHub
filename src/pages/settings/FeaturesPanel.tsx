import { Badge } from '@/components/ui/badge';
import { Switch } from '@/components/ui/switch';
import { useI18n } from '@/components/shared/LanguageProvider';
import { useSidebar } from '@/components/layout/SidebarContext';
import { OPTIONAL_NAV_IDS } from '@/lib/ui-preferences';
import { OPTIONAL_NAV_VISIBLE_COPY } from './settings-format';
import { SettingsGroup, SettingsRow } from './settings-shared';

/** 设置 → 功能：侧栏菜单显示，以及点「路由」时是否收起侧栏。 */
export function FeaturesPanel() {
  const { t } = useI18n();
  const { autoCollapseOnRoutes, setAutoCollapseOnRoutes, navVisible, setNavVisible } = useSidebar();

  return (
    <>
      <SettingsGroup first title={t('settings.general.sectionSidebarNav')} help="settings-nav-visible">
        {OPTIONAL_NAV_IDS.map((id) => {
          const copy = OPTIONAL_NAV_VISIBLE_COPY[id];
          const label = t(copy.label);
          return (
            <SettingsRow
              key={id}
              label={label}
              badge={
                id === 'plugins' ? (
                  <Badge variant="default">{t('common.inDevelopment')}</Badge>
                ) : undefined
              }
              description={t(copy.description)}
              descriptionTip={t(copy.tip)}
            >
              <Switch
                checked={navVisible[id]}
                onCheckedChange={(v) => setNavVisible(id, v)}
                aria-label={label}
              />
            </SettingsRow>
          );
        })}
      </SettingsGroup>
      <SettingsGroup title={t('settings.general.sectionSidebar')} help="settings-sidebar">
        <SettingsRow
          label={t('settings.general.autoCollapseOnRoutesLabel')}
          description={t('settings.general.autoCollapseOnRoutesDescription')}
          descriptionTip={t('settings.general.autoCollapseOnRoutesTip')}
        >
          <Switch
            checked={autoCollapseOnRoutes}
            onCheckedChange={setAutoCollapseOnRoutes}
            aria-label={t('settings.general.autoCollapseOnRoutesLabel')}
          />
        </SettingsRow>
      </SettingsGroup>
    </>
  );
}
