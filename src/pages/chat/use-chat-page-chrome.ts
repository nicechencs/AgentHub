import { useState } from 'react';

/**
 * Chat 页纯 UI 壳：侧栏开合、设置弹窗、危险确认、删除确认、侧栏查询。
 * 不含发送、取消、切会话、世代/单飞或票夹订阅。
 */
export function useChatPageChrome() {
  const [railOpen, setRailOpen] = useState(true);
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [dangerConfirm, setDangerConfirm] = useState(false);
  const [deleteConfirmId, setDeleteConfirmId] = useState<string | null>(null);
  const [railQuery, setRailQuery] = useState('');

  return {
    railOpen,
    setRailOpen,
    settingsOpen,
    setSettingsOpen,
    dangerConfirm,
    setDangerConfirm,
    deleteConfirmId,
    setDeleteConfirmId,
    railQuery,
    setRailQuery,
  };
}
