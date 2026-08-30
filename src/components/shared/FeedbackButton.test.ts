import { readFileSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { describe, expect, it } from 'vitest';
import { GITHUB_NEW_ISSUE_URL } from '@/lib/github';

const dir = path.dirname(fileURLToPath(import.meta.url));

function source(rel: string): string {
  return readFileSync(path.join(dir, rel), 'utf8');
}

describe('FeedbackButton', () => {
  it('opens the public new-issue form in the system browser', () => {
    const button = source('FeedbackButton.tsx');
    expect(button).toContain('GITHUB_NEW_ISSUE_URL');
    expect(button).toContain("from '@/lib/open-external'");
    expect(button).toContain('chrome.feedback.label');
    expect(GITHUB_NEW_ISSUE_URL).toBe('https://github.com/nicechencs/AgentHub/issues/new');
  });
});
