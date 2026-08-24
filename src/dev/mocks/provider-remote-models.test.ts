import { afterEach, describe, expect, it } from 'vitest';
import { createMockProviderPort, resetMockProviders } from './provider';

describe('mock listRemoteOpenAiModels', () => {
  afterEach(() => {
    resetMockProviders();
  });

  it('returns a canned list, empty list, or throws — never fetches', async () => {
    const port = createMockProviderPort();
    await expect(
      port.listRemoteOpenAiModels('https://mock-models.example.com', 'sk-test-abcdefgh'),
    ).resolves.toEqual(['mock-gpt-4', 'mock-gpt-4o-mini']);
    await expect(
      port.listRemoteOpenAiModels('https://empty-models.example.com', 'sk-test-abcdefgh'),
    ).resolves.toEqual([]);
    await expect(
      port.listRemoteOpenAiModels('https://fail-models.example.com', 'sk-test-abcdefgh'),
    ).rejects.toThrow(/remote models failed/);
    await expect(
      port.listRemoteOpenAiModels('https://other.example.com', 'sk-test-abcdefgh'),
    ).resolves.toEqual([]);
  });
});
