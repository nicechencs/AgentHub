import { describe, expect, it } from 'vitest';
import { createTranslator } from '@/lib/i18n';
import { inlineTerminalStatusText } from '@/components/shared/InlineTerminal';
import { localizeInstallCopy } from './install-labels';

describe('localizeInstallCopy', () => {
  const tEn = createTranslator('en');

  it('maps install / update / uninstall diagnoses to English', () => {
    expect(
      localizeInstallCopy(
        '诊断：该 Agent 没有脚本安装，已打开官网安装页。请完成安装后，完全退出并重启 AgentHub。',
        tEn,
      ),
    ).toBe(
      'This agent has no script install. The official setup page is open. Finish setup, then fully quit and restart AgentHub.',
    );
    expect(localizeInstallCopy('诊断：没有写入权限，不是 PATH 问题。', tEn)).toBe(
      'No write permission. This is not a PATH problem.',
    );
    expect(localizeInstallCopy('诊断：安装命令未成功退出（退出码 1）。', tEn)).toBe(
      'The install command did not exit successfully (exit code 1).',
    );
    expect(
      localizeInstallCopy('当前是 IDE 插件安装，无法在这里卸载程序，请到 IDE 插件中卸载', tEn),
    ).not.toMatch(/[\u4e00-\u9fff]/);
    expect(
      localizeInstallCopy('该 Agent 仅提供官网 Setup，无法自动检测更新', tEn),
    ).toBe('This agent only offers official Setup and cannot check for updates automatically.');
    expect(localizeInstallCopy('无法检测更新: network down', tEn)).toBe(
      "Couldn't check for updates: network down",
    );
    expect(localizeInstallCopy('Claude 卸载完成', tEn)).toBe('Claude uninstalled');
    expect(localizeInstallCopy('（已省略 8 行下载进度）', tEn)).toBe(
      '(8 download-progress lines omitted)',
    );
  });

  it('leaves raw Chinese when no translator is passed', () => {
    expect(localizeInstallCopy('诊断：安装超时。')).toBe('诊断：安装超时。');
  });
});

describe('inlineTerminalStatusText', () => {
  it('uses locale copy for running / failed / guided', () => {
    const tEn = createTranslator('en');
    expect(inlineTerminalStatusText('running', undefined, tEn)).toBe('In progress…');
    expect(inlineTerminalStatusText('running', 90, tEn)).toContain('waited 1m 30s');
    expect(inlineTerminalStatusText('failed', undefined, tEn)).toBe(
      'Failed. Try running the command above yourself',
    );
    expect(inlineTerminalStatusText('guided', undefined, tEn)).not.toMatch(/[\u4e00-\u9fff]/);
  });
});
