/**
 * Skills 页文案真源（Phase 2）。
 * L0/L1 用短词；L2 tip ≤2 短句；L3「单向投影」等不进主界面。
 * Toast 标题宜 ≤16 字；description 不塞完整路径。
 */
import type { SkillMapStatus, SkillSyncState } from '@/lib/types';
import { mapStatusLabel } from '@/lib/api/skill';

export const skillsCopy = {
  page: {
    title: '技能',
    descriptionTip: '单击技能名预览；格子启用到某工具。',
    meta: (shared: string | number, privateOnly: number) =>
      `共享库 ${shared} · 本工具 ${privateOnly}`,
    installCta: '安装',
  },

  tabs: {
    library: '本地共享库',
    market: '技能市场',
    libraryBadge: (n: number) => `${n} 个共享库技能`,
    privateBadge: (n: number) => `${n} 个只在本工具、可加入共享库`,
  },

  filters: {
    searchPlaceholder: '搜索技能名…',
    marketSearchPlaceholder: '搜索技能（如 react、pdf）…',
    enableAll: '全部',
    enablePrivate: '只在本工具',
    enableMapped: '已启用',
    enableUnmapped: '未启用',
    enableConflict: '冲突',
    batchEnable: '启用所选',
    batchEnableBusy: '启用中…',
    batchEnableHint: '启用所选到已装工具（跳过冲突）',
    clearSelection: '清除选择',
    selectedCount: (n: number) => `已选 ${n} 项`,
  },

  empty: {
    noMatchTitle: '没有匹配的技能',
    noMatchFilter: '试试调整搜索或过滤',
    noMatchLibrary: '共享库还没有技能，点右上角「安装」添加',
    clearFilter: '清除搜索与过滤',
    marketNoneTitle: '无市场结果',
    marketNoneDesc: '换关键词试试，或到设置切换市场源',
  },

  legend: {
    toggle: '图标说明',
    footer: '每列是一个工具；点格子启用或取消。',
    items: {
      linked: { label: '已启用（链接）', hint: '再点可取消' },
      copied: { label: '已启用（复制）', hint: '再点可取消' },
      absent: { label: '未启用', hint: '点击启用' },
      conflict: { label: '有冲突', hint: '点后确认是否覆盖' },
      blocked: { label: '不可用', hint: '未安装或不支持' },
    },
  },

  /** 矩阵格 L2 tip */
  cell: {
    tip(
      agentName: string,
      state: SkillSyncState,
      mapStatus: SkillMapStatus,
      linkKind?: string,
      reason?: string,
    ): string {
      switch (mapStatus) {
        case 'agent_unsupported':
          return reason
            ? `${agentName}：${reason}`
            : `${agentName}：不支持技能`;
        case 'agent_not_installed':
          return `${agentName}：未安装`;
        case 'target_unavailable':
          return `${agentName}：目录不可用`;
        case 'private_source':
          return '只在本工具 · 先加入共享库';
        case 'conflict':
          if (state === 'foreign') return '内容冲突 · 点击确认覆盖';
          if (state === 'conflict') return '状态不明 · 点击确认覆盖';
          return mapStatusLabel('conflict');
        case 'available':
          break;
      }
      switch (state) {
        case 'linked':
          return linkKind && linkKind !== 'none'
            ? `已启用（${linkKind}）· 点击取消`
            : '已启用 · 点击取消';
        case 'copied':
          return '已启用 · 点击取消';
        case 'absent':
          return '未启用 · 点击启用';
        case 'unsupported':
          return `${agentName}：不支持技能`;
        default:
          return state;
      }
    },
  },

  workspace: {
    privateTabTip: '只在本工具（可加入共享库）',
    selectedBar: (n: number) => `已选 ${n} 项（可加入共享库）`,
    batchAdopt: '批量加入共享库',
    batchAdoptBusy: '加入中…',
    selectAllHint: '全选可加入共享库的技能',
    selectAllEmpty: '当前没有可加入的项',
    alreadyInLibrary: '已在共享库，不可选',
    adopt: '加入共享库',
    adoptConflict: '覆盖加入共享库',
    adoptBusy: '加入中…',
    adoptHint: '加入共享库（保留本工具文件）',
    adoptConflictHint: '覆盖共享库（保留本工具文件）',
    inLibrary: '已在共享库',
    delete: '删除',
    remove: '从该工具目录删除',
    removeProjection: '仅从该工具移除（不删共享库）',
    removeAria: '删除此技能',
    removeProjectionAria: '从该工具目录移除',
    footerSelectable: (selectable: number, selected: number) =>
      `可加入 ${selectable} · 已选 ${selected}`,
    footerNone: '当前列表没有可加入共享库的项',
    emptyPrivateTitle: '没有可加入的技能',
    emptyPrivateDesc: (inLibrary: number) =>
      inLibrary > 0
        ? `另有 ${inLibrary} 个已在共享库，可切换筛选`
        : '本工具目录下暂无私有技能',
    emptyFilterTitle: '没有匹配',
    emptyFilterDesc: '调整筛选或搜索',
    viewInLibrary: '查看已在共享库',
    resetFilter: '重置筛选',
  },

  market: {
    /** 一行说明（不含外链，调用方把 home 链放在前面） */
    suffix: (isAuto: boolean) =>
      isAuto ? '自动 · 空搜热门 · 安装到共享库' : '空搜热门 · 安装到共享库',
    openDetail: '打开详情',
    openDetailHint: '在浏览器打开详情',
    install: '安装',
    installing: '安装中…',
    installed: '已安装',
    installHint: '安装到共享库',
    installedHint: '已在共享库',
  },

  menu: {
    preview: '预览 SKILL.md',
    openFolder: '打开所在文件夹',
  },

  preview: {
    collapse: '收起预览',
    openDir: '打开目录',
    modePreview: '预览',
    modeSource: '源码',
    sharedOrigin: '共享库',
    retry: '重试',
    truncated: '已截断',
  },

  dialog: {
    conflictTitle: '覆盖该工具里的同名技能？',
    conflictBody: (agentName: string, skillName: string) =>
      `${agentName} 已有「${skillName}」，且与共享库不同。覆盖后使用共享库版本。`,
    conflictConfirm: '覆盖并启用',
    conflictCancel: '取消',

    unsyncTitle: '取消在该工具的启用？',
    unsyncBody: (agentName: string, skillName: string) =>
      `将从 ${agentName} 取消「${skillName}」的启用。共享库与其他工具不受影响。`,
    unsyncKeep: '保持启用',
    unsyncConfirm: '取消启用',

    removeTitle: '从该工具目录移除？',
    deleteTitle: '删除该工具里的技能？',
    removeBody: (agentName: string, skillName: string) =>
      `将从 ${agentName} 移除「${skillName}」。共享库与其他工具不受影响。`,
    deleteBody: (agentName: string, skillName: string) =>
      `将删除 ${agentName} 中的「${skillName}」。尚未加入共享库，可能无法恢复。`,
    removeConfirm: '仅从该工具移除',
    deleteConfirm: '确认删除',
    busy: '处理中…',

    installTitle: '安装到共享库',
    installBody: '支持本地目录、.zip 或 git（需含 SKILL.md）。只写入共享库，不会自动启用。',
    installPlaceholder: 'C:\\path\\to\\skill  或  https://github.com/…/skill.git',
    installConfirm: '安装',

    importConflictTitle: '共享库内容将被覆盖',
    importConflictBody: (name: string) =>
      `「${name}」与共享库同名但内容不同。确认后用本工具版本覆盖共享库，原文件保留。`,
    importConflictConfirm: '覆盖加入',
  },

  toast: {
    enableFailed: (reason: string) => ({
      title: '无法启用',
      description: reason,
    }),
    disableFailed: (reason: string) => ({
      title: '无法取消启用',
      description: reason,
    }),
    enableOk: (agentName: string, skillName: string) => ({
      title: `已启用到 ${agentName}`,
      description: skillName,
    }),
    disableOk: (agentName: string, skillName: string) => ({
      title: `已取消 ${agentName} 的启用`,
      description: skillName,
    }),
    conflictPrompt: (agentName: string, skillName: string) => ({
      title: `覆盖 ${agentName} 中的同名技能？`,
      description: `「${skillName}」与共享库不同。覆盖后使用共享库版本。`,
      actionLabel: '覆盖并启用' as const,
    }),
    overwriteOk: (agentName: string, skillName: string) => ({
      title: `已启用到 ${agentName}`,
      description: skillName,
    }),
    overwriteFailed: (reason: string) => ({
      title: '无法覆盖启用',
      description: reason,
    }),
    installNeedSource: { title: '请输入路径或 git URL' },
    installOk: {
      title: '已加入共享库',
      description: '可在矩阵中启用到其他工具',
    },
    installFailed: (reason: string) => ({
      title: '安装失败',
      description: reason,
    }),
    openPathMissing: { title: '没有可打开的路径' },
    openPathFailed: (reason: string) => ({
      title: '打开目录失败',
      description: reason,
    }),
    noAgents: {
      title: '没有已安装的工具',
      description: '请先在 Agents 页安装 CLI',
    },
    batchEnable: (enabled: number, failed: number, skipParts: string[]) => ({
      title:
        enabled === 0 && failed === 0
          ? '所选技能无需启用'
          : `已启用所选 ${enabled} 项`,
      description:
        skipParts.length > 0
          ? `跳过：${skipParts.join(' · ')}。冲突请点格子处理。`
          : undefined,
    }),
    adoptOk: (overwrite: boolean) => ({
      title: overwrite ? '已覆盖并加入共享库' : '已加入共享库',
      description: '已可在矩阵中启用',
    }),
    adoptFailed: (reason: string) => ({
      title: '无法加入共享库',
      description: reason,
    }),
    batchAdopt: (ok: number, conflict: number, failed: number) => ({
      title: `批量加入完成：成功 ${ok}`,
      description:
        conflict > 0 || failed > 0
          ? `冲突 ${conflict} · 失败 ${failed}`
          : ok > 0
            ? '已可在矩阵中启用'
            : undefined,
    }),
    removeOk: (agentName: string, skillName: string) => ({
      title: `已从 ${agentName} 移除`,
      description: `「${skillName}」仅从该工具删除`,
    }),
    removeFailed: (reason: string) => ({
      title: '移除失败',
      description: reason,
    }),
    marketInstallOk: (name: string) => ({
      title: '已安装到共享库',
      description: `${name} · 到矩阵启用`,
      actionLabel: '去本地共享库' as const,
    }),
    marketExists: (reason: string) => ({
      title: '共享库已有同名技能',
      description: reason,
    }),
    openDetailFailed: (reason: string) => ({
      title: '无法打开详情页',
      description: reason,
    }),
  },
} as const;

export type SkillsCopy = typeof skillsCopy;
