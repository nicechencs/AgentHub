import { describe, expect, it } from 'vitest';
import { buildAgentInstallPreview, buildEnvInstallPreview } from './install-preview';

describe('buildInstallPreview', () => {
  it('builds agent install/upgrade preview without claiming success', () => {
    const install = buildAgentInstallPreview('claude', 'install', 'native');
    expect(install[0]).toContain('agenthub agent install claude');
    expect(install.join('\n')).not.toMatch(/✓|成功|完成/);

    const upgrade = buildAgentInstallPreview('codex', 'upgrade');
    expect(upgrade[0]).toContain('agenthub agent upgrade codex');
  });

  it('builds env install preview for winget nodejs and git', () => {
    const lines = buildEnvInstallPreview(['nodejs', 'npm', 'git'], 'winget');
    expect(lines.length).toBe(3);
    expect(lines[0]).toContain('OpenJS.NodeJS.LTS');
    expect(lines[1]).toContain('OpenJS.NodeJS.LTS');
    expect(lines[2]).toContain('Git.Git');
  });

  it('builds Homebrew previews for macOS runtimes', () => {
    const lines = buildEnvInstallPreview(['nodejs', 'npm', 'git'], 'brew');
    expect(lines[0]).toContain('brew install node');
    expect(lines[1]).toContain('brew install node');
    expect(lines[2]).toContain('brew install git');
    expect(lines.join('\n')).not.toContain('winget');
  });

  it('does not suggest installing PowerShell on non-Windows previews', () => {
    const lines = buildEnvInstallPreview(['powershell'], 'brew');
    expect(lines.join('\n')).not.toContain('brew install');
    expect(lines.join('\n').toLowerCase()).toContain('windows-only');
  });

  it('annotates upgrade with platform-aware underlying command', () => {
    const upgrade = buildAgentInstallPreview('claude', 'upgrade', 'native', 'macos');
    expect(upgrade[0]).toContain('agenthub agent upgrade claude');
    expect(upgrade.join('\n')).toMatch(/underlying \(macos\)/i);
  });

  it('handles empty targets', () => {
    expect(buildEnvInstallPreview([])).toEqual(['# no auto-install targets']);
  });

  it('builds copyable Linux remediations without winget or brew', () => {
    const lines = buildEnvInstallPreview(['nodejs', 'npm', 'git'], 'manual');
    expect(lines.join('\n')).toContain('apt-get install -y nodejs npm');
    expect(lines.join('\n')).toContain('apt-get install -y git');
    expect(lines.join('\n')).not.toContain('winget');
    expect(lines.join('\n')).not.toContain('brew install');
  });

  it('treats --channel apt as a Linux copy-command path, not winget/brew', () => {
    const lines = buildEnvInstallPreview(['nodejs', 'git'], 'apt');
    expect(lines.join('\n')).toContain('apt-get install');
    expect(lines.join('\n')).not.toContain('winget');
    expect(lines.join('\n')).not.toContain('brew install');
    expect(lines.join('\n')).not.toContain('$ agenthub env install');
  });
});
