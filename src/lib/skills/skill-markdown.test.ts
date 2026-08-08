import { describe, expect, it } from 'vitest';
import { skillMarkdownBody, splitSkillMarkdown } from './skill-markdown';

describe('splitSkillMarkdown', () => {
  it('strips simple name/description frontmatter and returns body', () => {
    const raw = [
      '---',
      'name: Preview Demo',
      'description: A short blurb for agents',
      '---',
      '',
      '# Hi',
      '',
      '**bold**',
      '',
    ].join('\n');

    const parts = splitSkillMarkdown(raw);
    expect(parts.hasFrontmatter).toBe(true);
    expect(parts.name).toBe('Preview Demo');
    expect(parts.description).toBe('A short blurb for agents');
    expect(parts.body).toBe('# Hi\n\n**bold**\n');
    expect(parts.body).not.toContain('---');
    expect(parts.body).not.toContain('description:');
  });

  it('collapses block scalar description', () => {
    const raw = [
      '---',
      'name: dbs',
      'description: |',
      '  Line one of the skill.',
      '  Line two continues.',
      '---',
      '',
      '## Steps',
      '',
    ].join('\n');

    const parts = splitSkillMarkdown(raw);
    expect(parts.description).toBe('Line one of the skill. Line two continues.');
    expect(parts.body.startsWith('## Steps')).toBe(true);
  });

  it('handles quoted values and BOM', () => {
    const raw = `\uFEFF---\nname: "Quoted Name"\ndescription: 'Quoted desc'\n---\n\nBody only.\n`;
    const parts = splitSkillMarkdown(raw);
    expect(parts.name).toBe('Quoted Name');
    expect(parts.description).toBe('Quoted desc');
    expect(parts.body).toBe('Body only.\n');
  });

  it('returns original content when frontmatter is missing', () => {
    const raw = '# Just a title\n\nNo yaml here.\n';
    const parts = splitSkillMarkdown(raw);
    expect(parts.hasFrontmatter).toBe(false);
    expect(parts.name).toBeNull();
    expect(parts.description).toBeNull();
    expect(parts.body).toBe(raw);
  });

  it('returns original when closing fence is missing', () => {
    const raw = '---\nname: broken\ndescription: no close\n\n# Body\n';
    const parts = splitSkillMarkdown(raw);
    expect(parts.hasFrontmatter).toBe(false);
    expect(parts.body).toBe(raw);
  });

  it('skillMarkdownBody is a convenience for body only', () => {
    expect(skillMarkdownBody('---\nname: x\n---\n\nHello')).toBe('Hello');
  });
});
