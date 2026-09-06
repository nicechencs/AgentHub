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
import { Hint } from '@/components/ui/tooltip';
import { ChatActionMenu } from './ChatActionMenu';
import type { ChatActionContext, ChatActionDef } from './chat-actions';
import type { RuntimeExtensionItem, RuntimeModelOption, RuntimeTurnSettings } from '@/lib/api/chat';

function extensionBadges(
  item: RuntimeExtensionItem,
  t: (key: never) => string,
): string[] {
  const badges: string[] = [];
  badges.push(item.installed ? t('chat.runtimeOps.installed' as never) : t('chat.runtimeOps.notInstalled' as never));
  if (item.enabled) badges.push(t('chat.runtimeOps.enabled' as never));
  else badges.push(t('chat.runtimeOps.disabledExt' as never));
  if (item.loaded) badges.push(t('chat.runtimeOps.loaded' as never));
  else badges.push(t('chat.runtimeOps.loadedUnknown' as never));
  if (item.kind === 'plugin') {
    badges.push(t('chat.runtimeOps.pluginStatusOnly' as never));
  } else if (item.callable) {
    badges.push(t('chat.runtimeOps.callable' as never));
  } else {
    badges.push(t('chat.runtimeOps.needPath' as never));
  }
  return badges;
}

export function ChatRuntimeExtras(props: {
  enabled: boolean;
  draft: string;
  commandSearchOpen: boolean;
  commandIndex?: number;
  actionContext: ChatActionContext;
  onRunAction: (action: ChatActionDef) => void;
  onHoverCommandIndex?: (index: number) => void;
  models: RuntimeModelOption[];
  settings: RuntimeTurnSettings;
  frozen: boolean;
  catalogLoading?: boolean;
  efforts: string[];
  onSwitchModel: (model: string) => void;
  onSwitchEffort: (effort: string) => void;
  images: string[];
  onAddImages: () => void;
  onRemoveImage: (path: string) => void;
  onPasteImages?: (files: File[]) => void;
  extensions: RuntimeExtensionItem[];
  selectedSkillIds: string[];
  onToggleSkill: (id: string) => void;
}) {
  const { t } = useI18n();
  const modelDisabledReason = props.frozen
    ? t('chat.runtimeOps.frozenDuringTurn')
    : props.catalogLoading
      ? t('chat.runtimeOps.catalogLoading')
      : props.models.length === 0
        ? t('chat.runtimeOps.catalogEmpty')
        : null;
  const effortDisabledReason = props.frozen
    ? t('chat.runtimeOps.frozenDuringTurn')
    : !props.settings.model
      ? t('chat.runtimeOps.needModelFirst')
      : props.efforts.length === 0
        ? t('chat.runtimeOps.effortUnavailable')
        : null;

  if (!props.enabled) {
    return (
      <div className="flex items-center gap-2 px-1 pb-1">
        <ChatActionMenu
          draft={props.draft}
          commandOpen={props.commandSearchOpen}
          selectedIndex={props.commandIndex}
          actionContext={props.actionContext}
          onRun={props.onRunAction}
          onHoverIndex={props.onHoverCommandIndex}
        />
      </div>
    );
  }

  return (
    <div
      className="space-y-2 px-1 pb-1"
      onPaste={(event) => {
        if (!props.onPasteImages) return;
        const files = Array.from(event.clipboardData?.files ?? []).filter((file) =>
          file.type.startsWith('image/'),
        );
        if (files.length === 0) return;
        event.preventDefault();
        props.onPasteImages(files);
      }}
    >
      <div className="flex flex-wrap items-center gap-2">
        <ChatActionMenu
          draft={props.draft}
          commandOpen={props.commandSearchOpen}
          selectedIndex={props.commandIndex}
          actionContext={props.actionContext}
          onRun={props.onRunAction}
          onHoverIndex={props.onHoverCommandIndex}
        />
        <Hint label={modelDisabledReason ?? undefined}>
          <DropdownMenu>
            <DropdownMenuTrigger asChild>
              <Button
                type="button"
                size="sm"
                variant="outline"
                disabled={Boolean(modelDisabledReason)}
                className="max-w-40"
              >
                <span className="truncate">
                  {props.settings.model || t('chat.composer.switchModel')}
                </span>
              </Button>
            </DropdownMenuTrigger>
            {props.models.length > 0 ? (
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
            ) : null}
          </DropdownMenu>
        </Hint>
        <Hint label={effortDisabledReason ?? undefined}>
          <DropdownMenu>
            <DropdownMenuTrigger asChild>
              <Button
                type="button"
                size="sm"
                variant="outline"
                disabled={Boolean(effortDisabledReason)}
              >
                {props.settings.effort || t('chat.runtimeOps.effort')}
              </Button>
            </DropdownMenuTrigger>
            {props.efforts.length > 0 ? (
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
            ) : null}
          </DropdownMenu>
        </Hint>
        <Hint label={t('chat.runtimeOps.pasteImageHint')}>
          <Button type="button" size="sm" variant="outline" onClick={props.onAddImages}>
            <ImagePlus className="mr-1 size-3.5" />
            {t('chat.runtimeOps.addImage')}
          </Button>
        </Hint>
        <span className="text-meta text-muted">{t('chat.runtimeOps.otherAttachmentsBlocked')}</span>
      </div>

      {props.images.length > 0 ? (
        <div className="flex flex-wrap gap-2">
          {props.images.map((path) => (
            <Hint key={path} label={path}>
              <span className="inline-flex max-w-full items-center gap-1 rounded-card border px-2 py-1 text-meta">
                <span className="truncate">{path.split(/[/\\]/).pop()}</span>
                <button type="button" aria-label={t('chat.runtimeOps.removeImage')} onClick={() => props.onRemoveImage(path)}>
                  <X className="size-3.5" />
                </button>
              </span>
            </Hint>
          ))}
        </div>
      ) : null}

      {props.extensions.length > 0 ? (
        <div className="rounded-card border p-2 text-meta">
          <div className="mb-1 font-medium">{t('chat.runtimeOps.extensions')}</div>
          <ul className="space-y-1">
            {props.extensions.map((item) => {
              const badges = extensionBadges(item, t as never);
              return (
                <li key={item.id} className="flex flex-wrap items-center gap-2">
                  <span className="font-medium">{item.name}</span>
                  <span className="text-muted">{item.kind === 'plugin' ? t('chat.runtimeOps.plugin') : t('chat.runtimeOps.skill')}</span>
                  <span className="text-muted">{badges.join(' · ')}</span>
                  {item.kind === 'skill' && item.callable ? (
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
