import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { useI18n } from '@/components/shared/LanguageProvider';
import { useToast } from '@/components/ui/toast';
import {
  pickChatImages,
  runtimeOptions,
  runtimeSetSettings,
  saveChatPasteImage,
  type RuntimeExtensionItem,
  type RuntimeModelOption,
  type RuntimeTurnSettings,
} from '@/lib/api/chat';
import type { Conversation } from '@/lib/types';
import {
  coerceSettingsToCatalog,
  effortsForModel,
  retainRuntimeCatalog,
  settingsForModelSwitch,
  type RuntimeCatalogMemory,
} from './chat-runtime-ops-model';

const IMAGE_EXT = /\.(png|jpe?g|gif|webp|bmp)$/i;
const MAX_IMAGES = 8;
const MAX_BYTES = 10 * 1024 * 1024;

function mimeToExt(mime: string): string | null {
  switch (mime.toLowerCase()) {
    case 'image/png':
      return 'png';
    case 'image/jpeg':
    case 'image/jpg':
      return 'jpg';
    case 'image/gif':
      return 'gif';
    case 'image/webp':
      return 'webp';
    case 'image/bmp':
    case 'image/x-ms-bmp':
      return 'bmp';
    default:
      return null;
  }
}

async function fileToBase64(file: File): Promise<string> {
  const buffer = await file.arrayBuffer();
  const bytes = new Uint8Array(buffer);
  let binary = '';
  const chunk = 0x8000;
  for (let i = 0; i < bytes.length; i += chunk) {
    binary += String.fromCharCode(...bytes.subarray(i, i + chunk));
  }
  return btoa(binary);
}

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
  const catalogRef = useRef<RuntimeCatalogMemory>({
    conversationId: null,
    models: [],
    extensions: [],
  });
  const settingsRef = useRef<RuntimeTurnSettings>({});
  settingsRef.current = settings;

  const refresh = useCallback(async () => {
    if (!active || !runtimeEnabled) {
      catalogRef.current = { conversationId: null, models: [], extensions: [] };
      setModels([]);
      setSettings({});
      setSettingsFrozen(false);
      setExtensions([]);
      return;
    }
    setLoading(true);
    try {
      const options = await runtimeOptions(active.id);
      const retained = retainRuntimeCatalog(catalogRef.current, options, active.id);
      catalogRef.current = {
        conversationId: active.id,
        models: retained.models,
        extensions: retained.extensions,
      };
      setModels(retained.models);
      const frozenNow = options.settingsFrozen || turnActive;
      // Idle: never keep an unsupported effort in controls. Frozen: show the
      // effective pair that started the turn (backend skips reconcile too).
      const incoming = options.settings ?? {};
      let nextSettings =
        !frozenNow && retained.models.length > 0
          ? coerceSettingsToCatalog(incoming, retained.models)
          : incoming;
      // Persist only when repairing an explicit unsupported effort. Do not
      // write merely because coerce filled a default for an omitted effort.
      const incomingEffort = incoming.effort?.trim() || null;
      const coercedEffort = nextSettings.effort?.trim() || null;
      if (
        !frozenNow &&
        retained.models.length > 0 &&
        incomingEffort &&
        incomingEffort !== coercedEffort
      ) {
        try {
          nextSettings = await runtimeSetSettings(active.id, nextSettings);
        } catch {
          // Keep local coerce even if persistence races; start still rejects.
        }
      }
      setSettings(nextSettings);
      setSettingsFrozen(frozenNow);
      setExtensions(retained.extensions);
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

  const currentEfforts = useMemo(
    () => effortsForModel(models, settings.model),
    [models, settings.model],
  );

  const switchModel = useCallback(
    async (model: string) => {
      if (!active || frozen) return;
      const prior = settingsRef.current;
      // Reset effort to the new model's default / first supported — never keep
      // an unsupported value from the previous model silently.
      const requested = settingsForModelSwitch(model, models);
      try {
        const next = await runtimeSetSettings(active.id, requested);
        setSettings(next);
      } catch (error) {
        setSettings(prior);
        toast({
          title: t('chat.runtimeOps.settingsReject'),
          description: error instanceof Error ? error.message : String(error),
          variant: 'danger',
        });
        await refresh();
      }
    },
    [active, frozen, models, refresh, t, toast],
  );

  const switchEffort = useCallback(
    async (effort: string) => {
      if (!active || frozen || !settings.model) return;
      const prior = settingsRef.current;
      try {
        const next = await runtimeSetSettings(active.id, {
          model: settings.model,
          effort,
        });
        setSettings(next);
      } catch (error) {
        setSettings(prior);
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

  const mergeImagePaths = useCallback(
    (paths: string[]) => {
      const next = [...images];
      for (const path of paths) {
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
      if (next.length > MAX_IMAGES) {
        toast({ title: t('chat.runtimeOps.tooManyImages'), variant: 'danger' });
        setImages(next.slice(0, MAX_IMAGES));
      } else {
        setImages(next);
      }
    },
    [images, t, toast],
  );

  const addImages = useCallback(async () => {
    if (!runtimeEnabled) return;
    try {
      const picked = await pickChatImages(t('chat.runtimeOps.pickImages'));
      mergeImagePaths(picked);
    } catch (error) {
      toast({
        title: t('chat.runtimeOps.pickImagesFail'),
        description: error instanceof Error ? error.message : String(error),
        variant: 'danger',
      });
    }
  }, [mergeImagePaths, runtimeEnabled, t, toast]);

  const pasteImages = useCallback(
    async (files: File[]) => {
      if (!runtimeEnabled || files.length === 0) return;
      const saved: string[] = [];
      for (const file of files) {
        const ext = mimeToExt(file.type) ?? (IMAGE_EXT.test(file.name) ? file.name.split('.').pop() : null);
        if (!ext) {
          toast({
            title: t('chat.runtimeOps.badImageType'),
            description: file.name || file.type,
            variant: 'danger',
          });
          continue;
        }
        if (file.size > MAX_BYTES) {
          toast({
            title: t('chat.runtimeOps.imageTooLarge'),
            description: file.name || file.type,
            variant: 'danger',
          });
          continue;
        }
        try {
          const base64 = await fileToBase64(file);
          const path = await saveChatPasteImage({
            base64,
            extension: ext,
            byteLength: file.size,
          });
          saved.push(path);
        } catch (error) {
          toast({
            title: t('chat.runtimeOps.pasteImageFail'),
            description: error instanceof Error ? error.message : String(error),
            variant: 'danger',
          });
        }
      }
      if (saved.length > 0) mergeImagePaths(saved);
    },
    [mergeImagePaths, runtimeEnabled, t, toast],
  );

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
    pasteImages,
    removeImage,
    clearAttachments,
    toggleSkill,
    startExtras,
    refresh,
  };
}
