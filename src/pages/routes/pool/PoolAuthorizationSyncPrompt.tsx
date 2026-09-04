import { useState } from 'react';
import { useI18n } from '@/components/shared/LanguageProvider';
import { Button } from '@/components/ui/button';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog';
import { useToast } from '@/components/ui/toast';
import { setSourceCustomModels } from '@/lib/api/adapter';
import type { SaveOauthPoolLoginResult } from './pool-authorization-edit';

export function PoolAuthorizationSyncPrompt({
  prompt,
  onFinish,
}: {
  prompt: SaveOauthPoolLoginResult | null;
  onFinish: (result: SaveOauthPoolLoginResult) => void;
}) {
  const { t } = useI18n();
  const { toast } = useToast();
  const [busy, setBusy] = useState(false);

  return (
    <Dialog open={Boolean(prompt)}>
      <DialogContent className="max-w-sm" hideClose>
        <DialogHeader>
          <DialogTitle>{t('routes.pool.page.syncToConnectionsTitle')}</DialogTitle>
          <DialogDescription>{t('routes.pool.page.syncToConnectionsDescription')}</DialogDescription>
        </DialogHeader>
        <DialogFooter>
          <Button
            type="button"
            variant="secondary"
            disabled={busy}
            onClick={() => {
              if (prompt) onFinish(prompt);
            }}
          >
            {t('routes.pool.page.syncToConnectionsSkip')}
          </Button>
          <Button
            type="button"
            disabled={busy}
            onClick={() => {
              if (!prompt) return;
              setBusy(true);
              void setSourceCustomModels(
                prompt.sourceKind,
                prompt.originalSourceId,
                prompt.models,
              )
                .catch((error) => {
                  toast({
                    title: t('routes.pool.page.syncToConnectionsFailed'),
                    description: error instanceof Error ? error.message : String(error),
                    variant: 'danger',
                  });
                })
                .finally(() => {
                  setBusy(false);
                  onFinish(prompt);
                });
            }}
          >
            {t('routes.pool.page.syncToConnectionsConfirm')}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
