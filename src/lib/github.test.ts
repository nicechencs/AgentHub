import { describe, expect, it } from 'vitest';
import { GITHUB_NEW_ISSUE_URL, GITHUB_REPO_URL } from './github';
import { isHttpUrl } from './open-external';

describe('GitHub public URLs', () => {
  it('opens the new-issue form under the public repository', () => {
    expect(isHttpUrl(GITHUB_REPO_URL)).toBe(true);
    expect(isHttpUrl(GITHUB_NEW_ISSUE_URL)).toBe(true);
    expect(GITHUB_REPO_URL).toBe('https://github.com/nicechencs/AgentHub');
    expect(GITHUB_NEW_ISSUE_URL).toBe('https://github.com/nicechencs/AgentHub/issues/new');
    expect(GITHUB_NEW_ISSUE_URL.startsWith(`${GITHUB_REPO_URL}/`)).toBe(true);
  });
});
