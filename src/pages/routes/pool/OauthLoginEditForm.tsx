import { useState } from 'react';
import { Plus, Trash2 } from 'lucide-react';
import { useI18n } from '@/components/shared/LanguageProvider';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Tip } from '@/components/ui/tooltip';
import { useToast } from '@/components/ui/toast';
import {
  forkConnectionAuthorization,
  setRouteAuthorizationPriority,
  setSourceCustomModels,
} from '@/lib/api/adapter';
import type { SourceModelCatalog } from '@/lib/backend/contracts/adapter';
import {
  saveOauthPoolLogin,
  type PoolOauthEditItem,
  type SaveOauthPoolLoginResult,
} from './pool-authorization-edit';

export function OauthLoginEditForm({
  item,
  catalog,
  onCancel,
  onSaved,
}: {
  item: PoolOauthEditItem;
  catalog?: SourceModelCatalog | null;
  onCancel: () => void;
  onSaved: (result: SaveOauthPoolLoginResult) => void;
}) {
  const { t } = useI18n();
  const { toast } = useToast();
  const [models, setModels] = useState<string[]>(() => [...(catalog?.models ?? [])]);
  const [customModel, setCustomModel] = useState('');
  const [priority, setPriority] = useState(
    item.priority == null || item.priority === 0 ? '' : String(item.priority),
  );
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const addCustomModel = () => {
    const next = customModel.trim();
    if (!next) return;
    setModels((current) => (current.includes(next) ? current : [...current, next]));
    setCustomModel('');
  };

  const save = async () => {
    setSaving(true);
    setError(null);
    try {
      const result = await saveOauthPoolLogin(
        { item, models, priority },
        {
          forkConnectionAuthorization,
          setSourceCustomModels,
          setRouteAuthorizationPriority,
        },
      );
      onSaved(result);
    } catch (cause) {
      const message = cause instanceof Error ? cause.message : String(cause);
      setError(message);
      toast({
        title: t('routes.pool.page.addFailed'),
        description: message,
        variant: 'danger',
      });
    } finally {
      setSaving(false);
    }
  };

  return (
    <div className="flex flex-col gap-3">
      <div className="flex flex-col gap-1.5">
        <span className="text-xs text-muted">{t('routes.pool.detail.models')}</span>
        {models.length > 0 ? (
          <div className="flex flex-col gap-1.5">
            {models.map((model) => (
              <div key={model} className="flex items-center gap-2">
                <div className="min-w-0 flex-1">
                  <Tip className="block truncate font-mono text-xs text-primary" label={model}>
                    {model}
                  </Tip>
                </div>
                <Button
                  type="button"
                  size="sm"
                  variant="ghost"
                  disabled={saving}
                  aria-label={t('routes.pool.page.apiModelsRemove')}
                  onClick={() => setModels((current) => current.filter((row) => row !== model))}
                >
                  <Trash2 className="h-3.5 w-3.5" />
                </Button>
              </div>
            ))}
          </div>
        ) : (
          <p className="text-meta text-muted">{t('routes.pool.page.apiModelsEmpty')}</p>
        )}
        <div className="flex items-center gap-2">
          <Input
            value={customModel}
            onChange={(event) => setCustomModel(event.target.value)}
            placeholder={t('routes.pool.page.apiModelsAddPlaceholder')}
            disabled={saving}
            onKeyDown={(event) => {
              if (event.key === 'Enter') {
                event.preventDefault();
                addCustomModel();
              }
            }}
          />
          <Button
            type="button"
            size="sm"
            variant="secondary"
            disabled={saving || !customModel.trim()}
            onClick={addCustomModel}
          >
            <Plus className="h-3.5 w-3.5" />
            {t('routes.pool.page.apiModelsAdd')}
          </Button>
        </div>
      </div>
      <label className="flex flex-col gap-1.5">
        <span className="text-xs text-muted">{t('routes.pool.detail.priority')}</span>
        <Input
          value={priority}
          onChange={(event) => setPriority(event.target.value)}
          placeholder={t('routes.pool.page.apiPriorityPlaceholder')}
          disabled={saving}
          inputMode="numeric"
        />
      </label>
      {error ? <p className="text-meta text-danger">{error}</p> : null}
      <div className="flex justify-end gap-2">
        <Button type="button" size="sm" variant="secondary" disabled={saving} onClick={onCancel}>
          {t('common.cancel')}
        </Button>
        <Button type="button" size="sm" disabled={saving} onClick={() => { void save(); }}>
          {t('common.save')}
        </Button>
      </div>
    </div>
  );
}
