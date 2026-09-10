import { useEffect, useState } from 'react';
import { ChevronDown, ImagePlus, MoreHorizontal, Sparkles, X } from 'lucide-react';
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
import type { RuntimeExtensionItem, RuntimeModelOption, RuntimeTurnSettings } from '@/lib/api/chat';
import { chatEffortHint, chatEffortLabel, chatModelDisplayName } from './chat-model-labels';

export function ChatRuntimeExtras(props: {
  enabled: boolean;
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
  /** Toolbar skill dropdown. Codex hides this — skills stay on `/` and auto-use. */
  showSkillPicker?: boolean;
  /** When `codex`, toolbar skill control is always hidden (defense if parent forgets the prop). */
  agentId?: string | null;
  inline?: boolean;
  compactSecondary?: boolean;
  modelMenuOpenNonce?: number;
}) {
  const { t } = useI18n();
  const callableSkills = props.extensions.filter((item) => item.kind === 'skill' && item.callable);
  // Codex: never mount the toolbar skill control (slash menu / auto-use only).
  const showSkillPicker =
    props.agentId !== 'codex' && props.showSkillPicker !== false;
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
  const modelTriggerHint = modelDisabledReason
    ?? `${t('chat.composer.switchModel')} · ${t('chat.composer.shortcutOpenModel')}`;

  if (!props.enabled) return null;

  // Inline mode uses `contents` so model/effort buttons sit in the composer
  // toolbar row. Once images are attached, switch to a column: `contents`
  // would drop the chip row into that same overflow-hidden horizontal flex
  // and the removable chips get clipped (true-window #312 FAIL).
  const rootClass = props.inline
    ? props.images.length > 0
      ? 'flex w-full min-w-0 flex-col gap-1.5'
      : 'contents'
    : 'space-y-2 px-1 pb-1';
  const controlsClass = props.inline && props.images.length === 0
    ? 'contents'
    : 'flex flex-wrap items-center gap-1.5';

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

      <div className={controlsClass} data-help="chat-composer-cluster">
        <Hint label={modelTriggerHint}>
          <DropdownMenu open={modelMenuOpen} onOpenChange={setModelMenuOpen}>
            <DropdownMenuTrigger asChild>
              <Button
                type="button"
                size="sm"
                variant="outline"
                disabled={Boolean(modelDisabledReason)}
                className="max-w-36"
                data-help="chat-model"
                aria-label={t('chat.composer.switchModel')}
                aria-keyshortcuts="Control+Shift+I"
              >
                <span className="min-w-0 truncate">
                  {props.settings.model
                    ? chatModelDisplayName(props.settings.model, t)
                    : t('chat.composer.switchModel')}
                </span>
                <ChevronDown className="h-3.5 w-3.5 shrink-0 opacity-60" />
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
        {props.efforts.length > 0 || (showSkillPicker && callableSkills.length > 0) || props.imageInput !== false ? (
          <Hint label={t('chat.composer.moreOptions')}>
            <DropdownMenu>
              <DropdownMenuTrigger asChild>
                <Button
                  type="button"
                  size="icon"
                  variant="ghost"
                  data-help="chat-composer-more"
                  aria-label={t('chat.composer.moreOptions')}
                  title={t('chat.composer.moreOptions')}
                >
                  <MoreHorizontal className="h-4 w-4" />
                </Button>
              </DropdownMenuTrigger>
              <DropdownMenuContent align="start" className="w-64">
                {props.efforts.length > 0 ? (
                  <>
                    <DropdownMenuLabel>{t('chat.runtimeOps.effort')}</DropdownMenuLabel>
                    <DropdownMenuRadioGroup
                      value={props.settings.effort ?? ''}
                      onValueChange={(id) => props.onSwitchEffort(id)}
                    >
                      {props.efforts.map((effort) => {
                        const hint = chatEffortHint(effort, t);
                        return (
                          <DropdownMenuRadioItem
                            key={effort}
                            value={effort}
                            disabled={Boolean(effortDisabledReason) || props.frozen}
                            data-help="chat-effort"
                          >
                            <span className="flex min-w-0 flex-1 items-baseline justify-between gap-3">
                              <span className="truncate">{chatEffortLabel(effort, t)}</span>
                              {hint ? <span className="shrink-0 text-meta text-muted">{hint}</span> : null}
                            </span>
                          </DropdownMenuRadioItem>
                        );
                      })}
                    </DropdownMenuRadioGroup>
                  </>
                ) : null}
                {showSkillPicker && callableSkills.length > 0 ? (
                  <>
                    {props.efforts.length > 0 ? <DropdownMenuSeparator /> : null}
                    <DropdownMenuLabel>{t('chat.runtimeOps.skill')}</DropdownMenuLabel>
                    {callableSkills.map((item) => (
                      <DropdownMenuItem key={item.id} onClick={() => props.onToggleSkill(item.id)}>
                        <span className="flex min-w-0 items-center gap-2">
                          <Sparkles className="size-3.5 shrink-0" />
                          <span className="w-4 shrink-0">{props.selectedSkillIds.includes(item.id) ? '✓' : ''}</span>
                          <span className="truncate">{item.name}</span>
                        </span>
                      </DropdownMenuItem>
                    ))}
                  </>
                ) : null}
                {props.imageInput !== false ? (
                  <>
                    {props.efforts.length > 0 || (showSkillPicker && callableSkills.length > 0) ? (
                      <DropdownMenuSeparator />
                    ) : null}
                    <DropdownMenuItem onClick={props.onAddImages}>
                      <ImagePlus className="mr-1 size-3.5" />
                      {`${t('chat.runtimeOps.addImage')}${props.images.length > 0 ? ` · ${props.images.length}` : ''}`}
                    </DropdownMenuItem>
                  </>
                ) : null}
              </DropdownMenuContent>
            </DropdownMenu>
          </Hint>
        ) : null}
        {!props.inline ? (
          <span className="text-meta text-muted">{t('chat.runtimeOps.otherAttachmentsBlocked')}</span>
        ) : null}
      </div>


      {showSkillPicker && !props.inline && props.selectedSkillIds.length > 0 ? (
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
