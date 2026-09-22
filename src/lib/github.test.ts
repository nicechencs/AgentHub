import { readFileSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { describe, expect, it } from 'vitest';
import { GITHUB_NEW_ISSUE_URL, GITHUB_REPO_URL } from './github';
import { isHttpUrl } from './open-external';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../..');

const productionRepoFiles = [
  'crates/agenthub-core/src/catalog/market.rs',
  'crates/agenthub-core/src/services/update_check_service.rs',
  'crates/agenthub-core/src/services/runtime_update_service.rs',
] as const;

describe('GitHub public URLs', () => {
  it('opens the new-issue form under the public repository', () => {
    expect(isHttpUrl(GITHUB_REPO_URL)).toBe(true);
    expect(isHttpUrl(GITHUB_NEW_ISSUE_URL)).toBe(true);
    expect(GITHUB_REPO_URL).toBe('https://github.com/nicechencs/AgentHub');
    expect(GITHUB_NEW_ISSUE_URL).toBe('https://github.com/nicechencs/AgentHub/issues/new');
    expect(GITHUB_NEW_ISSUE_URL.startsWith(`${GITHUB_REPO_URL}/`)).toBe(true);
  });

  it('locks the public repository slug in metadata, updater, and core', () => {
    expect(GITHUB_REPO_URL).toBe('https://github.com/nicechencs/AgentHub');
    expect(GITHUB_NEW_ISSUE_URL.startsWith(`${GITHUB_REPO_URL}/`)).toBe(true);

    const pkg = JSON.parse(readFileSync(path.join(root, 'package.json'), 'utf8')) as {
      repository: { url: string };
      bugs: { url: string };
      homepage: string;
    };
    expect(pkg.repository.url).toContain('github.com/nicechencs/AgentHub');
    expect(pkg.bugs.url).toContain('github.com/nicechencs/AgentHub');
    expect(pkg.homepage).toContain('github.com/nicechencs/AgentHub');

    const tauri = readFileSync(path.join(root, 'src-tauri/tauri.conf.json'), 'utf8');
    expect(tauri).toContain(
      'https://github.com/nicechencs/AgentHub/releases/latest/download/latest.json',
    );

    const repoRs = readFileSync(
      path.join(root, 'crates/agenthub-core/src/catalog/repo.rs'),
      'utf8',
    );
    expect(repoRs).toContain(
      'pub const GITHUB_REPOSITORY_URL: &str = "https://github.com/nicechencs/AgentHub";',
    );

    for (const rel of productionRepoFiles) {
      const text = readFileSync(path.join(root, rel), 'utf8');
      expect(text, rel).not.toContain('demo_chen/AgentHub');
      expect(text, rel).not.toContain('https://github.com/agenthub)');
      expect(text, rel).not.toContain('https://github.com/agenthub"');
    }

    const release = readFileSync(path.join(root, 'scripts/release-update.ps1'), 'utf8');
    expect(release).toContain('[string]$Repo = "nicechencs/AgentHub"');
  });
});
