import { ImagePlus, X } from 'lucide-react';
import { useI18n } from '@/components/shared/LanguageProvider';
import { Button } from '@/components/ui/button';
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuLabel,
  DropdownMenuRadioGroup,
  DropdownMenuRadioItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu';
import { ChatActionMenu } from './ChatActionMenu';
import type { ChatActionDef } from './chat-actions';
import type { RuntimeExtensionItem, RuntimeModelOption, RuntimeTurnSettings } from '@/lib/api/chat';

export function ChatRuntimeExtras(props: {
  enabled: boolean;
  draft: string;
  commandSearchOpen: boolean;
  onRunAction: (action: ChatActionDef) => void;
  models: RuntimeModelOption[];
  settings: RuntimeTurnSettings;
  frozen: boolean;
  efforts: string[];
  onSwitchModel: (model: string) => void;
  onSwitchEffort: (effort: string) => void;
  images: string[];
  onAddImages: () => void;
  onRemoveImage: (path: string) => void;
  extensions: RuntimeExtensionItem[];
  selectedSkillIds: string[];
  onToggleSkill: (id: string) => void;
}) {
  const { t } = useI18n();
  if (!props.enabled) {
    return (
      <div className="flex items-center gap-2 px-1 pb-1">
        <ChatActionMenu
          draft={props.draft}
          commandOpen={props.commandSearchOpen}
          onRun={props.onRunAction}
        />
      </div>
    );
  }

  return (
    <div className="space-y-2 px-1 pb-1">
      <div className="flex flex-wrap items-center gap-2">
        <ChatActionMenu
          draft={props.draft}
          commandOpen={props.commandSearchOpen}
          onRun={props.onRunAction}
        />
        {props.models.length > 0 ? (
          <DropdownMenu>
            <DropdownMenuTrigger asChild>
              <Button type="button" size="sm" variant="outline" disabled={props.frozen} className="max-w-40">
                <span className="truncate">{props.settings.model || t('chat.composer.switchModel')}</span>
              </Button>
            </DropdownMenuTrigger>
            <DropdownMenuContent align="start" className="w-64">
              <DropdownMenuLabel>{t('chat.composer.switchModel')}</DropdownMenuLabel>
              <DropdownMenuSeparator />
              <DropdownMenuRadioGroup
                value={props.settings.model ?? ''}
                onValueChange={(id) => props.onSwitchModel(id)}
              >
                {props.models.map((model) => (
                  <DropdownMenuRadioItem key={model.id} value={model.id} disabled={props.frozen}>
                    {model.id}
                  </DropdownMenuRadioItem>
                ))}
              </DropdownMenuRadioGroup>
            </DropdownMenuContent>
          </DropdownMenu>
        ) : null}
        {props.efforts.length > 0 ? (
          <DropdownMenu>
            <DropdownMenuTrigger asChild>
              <Button type="button" size="sm" variant="outline" disabled={props.frozen || !props.settings.model}>
                {props.settings.effort || t('chat.runtimeOps.effort')}
              </Button>
            </DropdownMenuTrigger>
            <DropdownMenuContent align="start">
              <DropdownMenuLabel>{t('chat.runtimeOps.effort')}</DropdownMenuLabel>
              <DropdownMenuSeparator />
              <DropdownMenuRadioGroup
                value={props.settings.effort ?? ''}
                onValueChange={(id) => props.onSwitchEffort(id)}
              >
                {props.efforts.map((effort) => (
                  <DropdownMenuRadioItem key={effort} value={effort} disabled={props.frozen}>
                    {effort}
                  </DropdownMenuRadioItem>
                ))}
              </DropdownMenuRadioGroup>
            </DropdownMenuContent>
          </DropdownMenu>
        ) : null}
        <Button type="button" size="sm" variant="outline" onClick={props.onAddImages}>
          <ImagePlus className="mr-1 size-3.5" />
          {t('chat.runtimeOps.addImage')}
        </Button>
      </div>

      {props.images.length > 0 ? (
        <div className="flex flex-wrap gap-2">
          {props.images.map((path) => (
            <span
              key={path}
              className="inline-flex max-w-full items-center gap-1 rounded-md border px-2 py-1 text-meta"
              title={path}
            >
              <span className="truncate">{path.split(/[/\\]/).pop()}</span>
              <button type="button" aria-label={t('chat.runtimeOps.removeImage')} onClick={() => props.onRemoveImage(path)}>
                <X className="size-3.5" />
              </button>
            </span>
          ))}
        </div>
      ) : null}

      {props.extensions.length > 0 ? (
        <div className="rounded-md border p-2 text-meta">
          <div className="mb-1 font-medium">{t('chat.runtimeOps.extensions')}</div>
          <ul className="space-y-1">
            {props.extensions.map((item) => {
              const badges = [
                item.installed ? t('chat.runtimeOps.installed') : null,
                item.enabled ? t('chat.runtimeOps.enabled') : null,
                item.loaded ? t('chat.runtimeOps.loaded') : null,
                item.callable ? t('chat.runtimeOps.callable') : t('chat.runtimeOps.notCallable'),
              ].filter(Boolean);
              return (
                <li key={item.id} className="flex flex-wrap items-center gap-2">
                  <span className="font-medium">{item.name}</span>
                  <span className="text-muted">{badges.join(' · ')}</span>
                  {item.callable ? (
                    <Button
                      type="button"
                      size="sm"
                      variant={props.selectedSkillIds.includes(item.id) ? 'default' : 'outline'}
                      onClick={() => props.onToggleSkill(item.id)}
                    >
                      {t('chat.runtimeOps.useForTurn')}
                    </Button>
                  ) : null}
                </li>
              );
            })}
          </ul>
        </div>
      ) : null}
    </div>
  );
}
