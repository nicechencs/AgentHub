import { useEffect, useState } from 'react';
import { ImagePlus, X } from 'lucide-react';
import { useI18n } from '@/components/shared/LanguageProvider';
import { Button } from '@/components/ui/button';
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
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
import { chatEffortHint, chatEffortLabel, chatModelDisplayName } from './chat-model-labels';

export function ChatRuntimeExtras(props: {
  enabled: boolean;
  draft: string;
  commandSearchOpen: boolean;
  commandIndex?: number;
  actionContext: ChatActionContext;
  extraActions?: ChatActionDef[];
  onRunAction: (action: ChatActionDef) => void;
  onHoverCommandIndex?: (index: number) => void;
  models: RuntimeModelOption[];
  settings: RuntimeTurnSettings;
  frozen: boolean;
  frozenReason?: string;
  catalogLoading?: boolean;
  efforts: string[];
  onSwitchModel: (model: string) => void;
  onSwitchEffort: (effort: string) => void;
  images: string[];
  imageInput?: boolean;
  onAddImages: () => void;
  onRemoveImage: (path: string) => void;
  onPasteImages?: (files: File[]) => void;
  extensions: RuntimeExtensionItem[];
  selectedSkillIds: string[];
  onToggleSkill: (id: string) => void;
  inline?: boolean;
  modelMenuOpenNonce?: number;
}) {
  const { t } = useI18n();
  const callableSkills = props.extensions.filter((item) => item.kind === 'skill' && item.callable);
  const modelDisabledReason = props.frozen
    ? props.frozenReason ?? t('chat.runtimeOps.frozenDuringTurn')
    : props.catalogLoading
      ? t('chat.runtimeOps.catalogLoading')
      : props.models.length === 0
        ? t('chat.runtimeOps.catalogEmpty')
        : null;
  const effortDisabledReason = props.frozen
    ? props.frozenReason ?? t('chat.runtimeOps.frozenDuringTurn')
    : !props.settings.model
      ? t('chat.runtimeOps.needModelFirst')
      : props.efforts.length === 0
        ? t('chat.runtimeOps.effortUnavailable')
        : null;
  const [modelMenuOpen, setModelMenuOpen] = useState(false);
  useEffect(() => {
    if (!props.modelMenuOpenNonce) return;
    if (modelDisabledReason || props.models.length === 0) return;
    setModelMenuOpen(true);
  }, [modelDisabledReason, props.modelMenuOpenNonce, props.models.length]);
  const currentEffortHint = props.settings.effort
    ? chatEffortHint(props.settings.effort, t)
    : null;
  const modelTriggerHint = modelDisabledReason
    ?? `${t('chat.composer.switchModel')} · ${t('chat.composer.shortcutOpenModel')}`;

  if (!props.enabled) {
    return (
      <div className="flex items-center gap-2 px-1 pb-1">
        <ChatActionMenu
          draft={props.draft}
          commandOpen={props.commandSearchOpen}
          selectedIndex={props.commandIndex}
          actionContext={props.actionContext}
          extraActions={props.extraActions}
          onRun={props.onRunAction}
          onHoverIndex={props.onHoverCommandIndex}
        />
      </div>
    );
  }

  // Inline mode uses `contents` so model/effort buttons sit in the composer
  // toolbar row. Once images are attached, switch to a column: `contents`
  // would drop the chip row into that same overflow-hidden horizontal flex
  // and the removable chips get clipped (true-window #312 FAIL).
  const rootClass = props.inline
    ? props.images.length > 0
      ? 'flex w-full min-w-0 flex-col gap-2'
      : 'contents'
    : 'space-y-2 px-1 pb-1';

  return (
    <div
      className={rootClass}
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
      {props.images.length > 0 ? (
        <div className="flex flex-wrap gap-2" data-help="chat-image-chips">
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

      <div className="flex flex-wrap items-center gap-2">
        <ChatActionMenu
          draft={props.draft}
          commandOpen={props.commandSearchOpen}
          selectedIndex={props.commandIndex}
          actionContext={props.actionContext}
          extraActions={props.extraActions}
          onRun={props.onRunAction}
          onHoverIndex={props.onHoverCommandIndex}
        />
        <Hint label={modelTriggerHint}>
          <DropdownMenu open={modelMenuOpen} onOpenChange={setModelMenuOpen}>
            <DropdownMenuTrigger asChild>
              <Button
                type="button"
                size="sm"
                variant="outline"
                disabled={Boolean(modelDisabledReason)}
                className="max-w-48"
                data-help="chat-model"
                aria-label={t('chat.composer.switchModel')}
                aria-keyshortcuts="Control+Shift+I"
              >
                <span className="truncate">
                  {props.settings.model
                    ? chatModelDisplayName(props.settings.model, t)
                    : t('chat.composer.switchModel')}
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
                      <span className="truncate">{chatModelDisplayName(model.id, t)}</span>
                    </DropdownMenuRadioItem>
                  ))}
                </DropdownMenuRadioGroup>
              </DropdownMenuContent>
            ) : null}
          </DropdownMenu>
        </Hint>
        <Hint label={effortDisabledReason ?? currentEffortHint ?? undefined}>
          <DropdownMenu>
            <DropdownMenuTrigger asChild>
              <Button
                type="button"
                size="sm"
                variant="outline"
                disabled={Boolean(effortDisabledReason)}
                data-help="chat-effort"
                aria-label={t('chat.runtimeOps.effort')}
              >
                {props.settings.effort
                  ? chatEffortLabel(props.settings.effort, t)
                  : t('chat.runtimeOps.effort')}
              </Button>
            </DropdownMenuTrigger>
            {props.efforts.length > 0 ? (
              <DropdownMenuContent align="start" className="w-56">
                <DropdownMenuLabel>{t('chat.runtimeOps.effort')}</DropdownMenuLabel>
                <DropdownMenuSeparator />
                <DropdownMenuRadioGroup
                  value={props.settings.effort ?? ''}
                  onValueChange={(id) => props.onSwitchEffort(id)}
                >
                  {props.efforts.map((effort) => {
                    const hint = chatEffortHint(effort, t);
                    return (
                      <DropdownMenuRadioItem key={effort} value={effort} disabled={props.frozen}>
                        <span className="flex min-w-0 flex-1 items-baseline justify-between gap-3">
                          <span className="truncate">{chatEffortLabel(effort, t)}</span>
                          {hint ? <span className="shrink-0 text-meta text-muted">{hint}</span> : null}
                        </span>
                      </DropdownMenuRadioItem>
                    );
                  })}
                </DropdownMenuRadioGroup>
              </DropdownMenuContent>
            ) : null}
          </DropdownMenu>
        </Hint>
        {!effortDisabledReason && currentEffortHint ? (
          <span className="text-meta text-muted">{currentEffortHint}</span>
        ) : null}
        {callableSkills.length > 0 ? (
          <DropdownMenu>
            <DropdownMenuTrigger asChild>
              <Button type="button" size="sm" variant="outline" className="max-w-32">
                <span className="truncate">
                  {t('chat.runtimeOps.skill')}
                  {props.selectedSkillIds.length > 0 ? ` · ${props.selectedSkillIds.length}` : ''}
                </span>
              </Button>
            </DropdownMenuTrigger>
            <DropdownMenuContent align="start" className="w-64">
              <DropdownMenuLabel>{t('chat.runtimeOps.skill')}</DropdownMenuLabel>
              <DropdownMenuSeparator />
              {callableSkills.map((item) => (
                <DropdownMenuItem key={item.id} onClick={() => props.onToggleSkill(item.id)}>
                  <span className="flex min-w-0 items-center gap-2">
                    <span className="w-4 shrink-0">{props.selectedSkillIds.includes(item.id) ? '✓' : ''}</span>
                    <span className="truncate">{item.name}</span>
                  </span>
                </DropdownMenuItem>
              ))}
            </DropdownMenuContent>
          </DropdownMenu>
        ) : null}
        {props.imageInput !== false ? (
        <Hint label={t('chat.runtimeOps.pasteImageHint')}>
          <Button type="button" size="sm" variant="outline" onClick={props.onAddImages}>
            <ImagePlus className="mr-1 size-3.5" />
            {t('chat.runtimeOps.addImage')}{props.images.length > 0 ? ` · ${props.images.length}` : ''}
          </Button>
        </Hint>
        ) : null}
        {!props.inline ? (
          <span className="text-meta text-muted">{t('chat.runtimeOps.otherAttachmentsBlocked')}</span>
        ) : null}
      </div>


      {!props.inline && props.selectedSkillIds.length > 0 ? (
        <div className="flex flex-wrap gap-2 text-meta">
          {props.selectedSkillIds.map((id) => {
            const item = props.extensions.find((extension) => extension.id === id);
            return (
              <span key={id} className="inline-flex max-w-full items-center gap-1 rounded-card border px-2 py-1">
                <span className="truncate">
                  {t('chat.runtimeOps.skill')} · {item?.name ?? id}
                </span>
                <button type="button" aria-label={t('chat.runtimeOps.removeSkill')} onClick={() => props.onToggleSkill(id)}>
                  <X className="size-3.5" />
                </button>
              </span>
            );
          })}
        </div>
      ) : null}
    </div>
  );
}
