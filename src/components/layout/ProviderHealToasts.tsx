import { useEffect } from 'react';
import { useNavigate } from 'react-router-dom';
import { useI18n } from '@/components/shared/LanguageProvider';
import { useToast } from '@/components/ui/toast';
import { onProviderBindingHeal } from '@/lib/backend/tauri/provider-heal-events';
import { logger } from '@/lib/logger';

/** App-shell toast for live-vs-pool login heal / conflict notices. */
export function ProviderHealToasts() {
  const { t } = useI18n();
  const { toast } = useToast();
  const navigate = useNavigate();

  useEffect(() => {
    let cancelled = false;
    let unsub: (() => void) | undefined;
    void onProviderBindingHeal((payload) => {
      if (payload.kind === 'conflict') {
        const agent = payload.agent.trim();
        toast({
          title: t('connections.healConflict'),
          variant: 'warning',
          actionLabel: t('connections.healConflictAction'),
          onAction: () => {
            if (!agent) {
              navigate('/connections');
              return;
            }
            navigate(`/connections?agent=${encodeURIComponent(agent)}`);
          },
        });
        return;
      }
      toast({ title: t('connections.healAligned'), variant: 'success' });
    })
      .then((fn) => {
        if (cancelled) {
          fn();
          return;
        }
        unsub = fn;
      })
      .catch((error) => {
        logger.scope('providers').error('provider binding heal subscription unavailable', error);
      });
    return () => {
      cancelled = true;
      unsub?.();
    };
  }, [t, toast, navigate]);

  return null;
}
