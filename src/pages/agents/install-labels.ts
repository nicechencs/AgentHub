import type { TranslateFn } from '@/lib/i18n';

/** Backend install/update/uninstall strings stay Chinese. Remap at display time. */
export function localizeInstallCopy(raw: string, t?: TranslateFn): string {
  if (!t || !raw) return raw;
  const trimmed = raw.trim();
  if (!trimmed) return trimmed;

  if (trimmed.includes('没有脚本安装') && trimmed.includes('官网安装页')) {
    return t('agents.installCopy.setupGuideDiagnosis');
  }
  const setupOpened = trimmed.match(/^(.+) 已打开官网安装页，请完成安装后重启 AgentHub$/);
  if (setupOpened) {
    return t('agents.installCopy.setupGuideOpened', { name: setupOpened[1] });
  }
  if (trimmed.includes('没有写入权限') && trimmed.includes('PATH')) {
    return t('agents.installCopy.noWritePermission');
  }
  if (trimmed.includes('安装超时')) return t('agents.installCopy.timedOut');
  if (trimmed.includes('安装命令无法启动')) return t('agents.installCopy.cannotStartCommand');
  const exit = trimmed.match(/安装命令未成功退出（退出码 (.+)）/);
  if (exit) return t('agents.installCopy.exitCode', { code: exit[1] });

  if (trimmed.includes('IDE 插件安装') && trimmed.includes('无法在这里卸载程序')) {
    return t('agents.installCopy.ideUninstall');
  }
  if (trimmed.includes('桌面应用安装') && trimmed.includes('无法在这里卸载程序')) {
    return t('agents.installCopy.desktopUninstall');
  }
  if (trimmed.includes('IDE 插件安装') && trimmed.includes('将仅清理配置目录')) {
    return t('agents.installCopy.ideUninstallPurge');
  }
  if (trimmed.includes('桌面应用安装') && trimmed.includes('将仅清理配置目录')) {
    return t('agents.installCopy.desktopUninstallPurge');
  }
  if (trimmed.includes('IDE 插件安装') && trimmed.includes('无法在这里更新')) {
    return t('agents.installCopy.ideUpdate');
  }
  if (trimmed.includes('桌面应用安装') && trimmed.includes('无法在这里更新')) {
    return t('agents.installCopy.desktopUpdate');
  }

  if (trimmed.includes('仅提供官网 Setup') && trimmed.includes('无法自动检测更新')) {
    return t('agents.installCopy.setupOnly');
  }
  const detect = trimmed.match(/^无法检测更新(?::\s*(.+))?$/);
  if (detect) {
    return detect[1]
      ? t('agents.installCopy.cannotDetect', { error: detect[1] })
      : t('agents.installCopy.cannotDetectShort');
  }
  if (trimmed.includes('当前安装来自') && trimmed.includes('IDE 插件')) {
    return t('agents.installCopy.fromIde');
  }
  if (trimmed.includes('当前安装来自') && trimmed.includes('桌面应用')) {
    return t('agents.installCopy.fromDesktop');
  }
  const channelNpm = trimmed.match(/^当前安装渠道为 (.+)，已对照 npm dist-tag/);
  if (channelNpm) {
    return t('agents.installCopy.channelNpm', { channel: channelNpm[1] });
  }
  const channelOfficial = trimmed.match(/^当前安装渠道为 (.+)，已对照官方版本源（(.+)）/);
  if (channelOfficial) {
    return t('agents.installCopy.channelOfficial', {
      channel: channelOfficial[1],
      source: channelOfficial[2],
    });
  }

  const deletedConfig = trimmed.match(/^(.+) 已删除配置；程序仍在 IDE 插件或桌面应用中$/);
  if (deletedConfig) {
    return t('agents.installCopy.deletedConfigKeptApp', { name: deletedConfig[1] });
  }
  const uninstallDone = trimmed.match(/^(.+) 卸载完成$/);
  if (uninstallDone) {
    return t('agents.installCopy.uninstallDone', { name: uninstallDone[1] });
  }
  const stillDetected = trimmed.match(/^(.+) 卸载后仍检测到二进制/);
  if (stillDetected) {
    return t('agents.installCopy.uninstallStillDetected', { name: stillDetected[1] });
  }
  const programFailed = trimmed.match(/^(.+) 未能自动卸载程序本体$/);
  if (programFailed) {
    return t('agents.installCopy.uninstallProgramFailed', { name: programFailed[1] });
  }

  const omittedProgress = trimmed.match(/^（已省略 (\d+) 行下载进度）$/);
  if (omittedProgress) {
    return t('agents.installCopy.omittedProgress', { n: omittedProgress[1] });
  }
  const omittedOutput = trimmed.match(/^（已省略 (\d+) 行安装输出）$/);
  if (omittedOutput) {
    return t('agents.installCopy.omittedOutput', { n: omittedOutput[1] });
  }

  return trimmed;
}
