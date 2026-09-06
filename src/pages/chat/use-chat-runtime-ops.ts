import { useCallback, useEffect, useMemo, useState } from 'react';
import { useI18n } from '@/components/shared/LanguageProvider';
import { useToast } from '@/components/ui/toast';
import {
  pickChatImages,
  runtimeOptions,
  runtimeSetSettings,
  type RuntimeExtensionItem,
  type RuntimeModelOption,
  type RuntimeTurnSettings,
} from '@/lib/api/chat';
import type { Conversation } from '@/lib/types';

const IMAGE_EXT = /\.(png|jpe?g|gif|webp|bmp)$/i;

export function useChatRuntimeOps(input: {
  active: Conversation | null;
  runtimeEnabled: boolean;
  turnActive: boolean;
}) {
  const { t } = useI18n();
  const { toast } = useToast();
  const { active, runtimeEnabled, turnActive } = input;

  const [models, setModels] = useState<RuntimeModelOption[]>([]);
  const [settings, setSettings] = useState<RuntimeTurnSettings>({});
  const [settingsFrozen, setSettingsFrozen] = useState(false);
  const [extensions, setExtensions] = useState<RuntimeExtensionItem[]>([]);
  const [images, setImages] = useState<string[]>([]);
  const [selectedSkillIds, setSelectedSkillIds] = useState<string[]>([]);
  const [loading, setLoading] = useState(false);

  const refresh = useCallback(async () => {
    if (!active || !runtimeEnabled) {
      setModels([]);
      setSettings({});
      setSettingsFrozen(false);
      setExtensions([]);
      return;
    }
    setLoading(true);
    try {
      const options = await runtimeOptions(active.id);
      setModels(options.models);
      setSettings(options.settings ?? {});
      setSettingsFrozen(options.settingsFrozen || turnActive);
      setExtensions(options.extensions);
    } catch (error) {
      toast({
        title: t('chat.runtimeOps.optionsFail'),
        description: error instanceof Error ? error.message : String(error),
        variant: 'danger',
      });
    } finally {
      setLoading(false);
    }
  }, [active, runtimeEnabled, t, toast, turnActive]);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  // Draft images/skills survive model switch; clear only when conversation changes.
  useEffect(() => {
    setImages([]);
    setSelectedSkillIds([]);
  }, [active?.id]);

  const frozen = settingsFrozen || turnActive;

  const currentEfforts = useMemo(() => {
    const model = settings.model;
    if (!model) return [] as string[];
    return models.find((item) => item.id === model)?.efforts ?? [];
  }, [models, settings.model]);

  const switchModel = useCallback(
    async (model: string) => {
      if (!active || frozen) return;
      try {
        const next = await runtimeSetSettings(active.id, {
          model,
          effort: undefined,
        });
        setSettings(next);
      } catch (error) {
        toast({
          title: t('chat.runtimeOps.settingsReject'),
          description: error instanceof Error ? error.message : String(error),
          variant: 'danger',
        });
        await refresh();
      }
    },
    [active, frozen, refresh, t, toast],
  );

  const switchEffort = useCallback(
    async (effort: string) => {
      if (!active || frozen || !settings.model) return;
      try {
        const next = await runtimeSetSettings(active.id, {
          model: settings.model,
          effort,
        });
        setSettings(next);
      } catch (error) {
        toast({
          title: t('chat.runtimeOps.settingsReject'),
          description: error instanceof Error ? error.message : String(error),
          variant: 'danger',
        });
        await refresh();
      }
    },
    [active, frozen, refresh, settings.model, t, toast],
  );

  const addImages = useCallback(async () => {
    if (!runtimeEnabled) return;
    try {
      const picked = await pickChatImages(t('chat.runtimeOps.pickImages'));
      const next = [...images];
      for (const path of picked) {
        if (!IMAGE_EXT.test(path)) {
          toast({
            title: t('chat.runtimeOps.badImageType'),
            description: path,
            variant: 'danger',
          });
          continue;
        }
        if (!next.includes(path)) next.push(path);
      }
      if (next.length > 8) {
        toast({ title: t('chat.runtimeOps.tooManyImages'), variant: 'danger' });
        setImages(next.slice(0, 8));
      } else {
        setImages(next);
      }
    } catch (error) {
      toast({
        title: t('chat.runtimeOps.pickImagesFail'),
        description: error instanceof Error ? error.message : String(error),
        variant: 'danger',
      });
    }
  }, [images, runtimeEnabled, t, toast]);

  const removeImage = useCallback((path: string) => {
    setImages((prev) => prev.filter((item) => item !== path));
  }, []);

  const clearAttachments = useCallback(() => {
    setImages([]);
    setSelectedSkillIds([]);
  }, []);

  const toggleSkill = useCallback((id: string) => {
    setSelectedSkillIds((prev) =>
      prev.includes(id) ? prev.filter((item) => item !== id) : [...prev, id],
    );
  }, []);

  const startExtras = useMemo(() => {
    const skills = extensions
      .filter((item) => item.kind === 'skill' && item.callable && selectedSkillIds.includes(item.id))
      .map((item) => ({
        name: item.name,
        path: item.path || item.id,
      }));
    return {
      images: images.map((path) => ({ path })),
      skills,
    };
  }, [extensions, images, selectedSkillIds]);

  return {
    loading,
    models,
    settings,
    frozen,
    currentEfforts,
    extensions,
    images,
    selectedSkillIds,
    switchModel,
    switchEffort,
    addImages,
    removeImage,
    clearAttachments,
    toggleSkill,
    startExtras,
    refresh,
  };
}
