import { describe, expect, it } from 'vitest';
import { createTranslator } from '@/lib/i18n';
import {
  PROJECT_CARD_COLUMN_SPECS,
  PROJECT_CARD_RESIZE_COLUMNS,
  projectCardColumnLabel,
  projectGroupListTemplate,
} from './project-card-columns';

const t = createTranslator('zh');

describe('project card columns', () => {
  it('stores pixel widths for name and path; time / sessions / size hug content', () => {
    expect(PROJECT_CARD_RESIZE_COLUMNS).toEqual(['name', 'path']);
    expect(PROJECT_CARD_COLUMN_SPECS.map((spec) => spec.key)).toEqual(['name', 'path']);
  });

  it('builds one template so every card shares the same tracks', () => {
    const template = projectGroupListTemplate({
      name: 280,
      path: 180,
    });
    expect(template).toBe(
      '2rem 280px 180px max-content max-content max-content minmax(0,1fr) 2.5rem',
    );
  });

  it('labels resize handles with the field names on the card', () => {
    expect(projectCardColumnLabel('name', t)).toBe('名称');
    expect(projectCardColumnLabel('path', t)).toBe('路径');
  });
});
