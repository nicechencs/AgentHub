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

describe('mock listRemoteOpenAiModelsForProvider', () => {
  afterEach(() => {
    resetMockProviders();
  });

  it('resolves by provider id + baseUrl and never takes a raw key', async () => {
    const port = createMockProviderPort();
    await expect(
      port.listRemoteOpenAiModelsForProvider(
        'p-mock-openrouter',
        'https://openrouter.ai/api/v1',
      ),
    ).resolves.toEqual(['mock-gpt-4', 'mock-gpt-4o-mini']);
    await expect(
      port.listRemoteOpenAiModelsForProvider('p-empty-relay', 'https://relay.example.com/v1'),
    ).resolves.toEqual([]);
    await expect(
      port.listRemoteOpenAiModelsForProvider('p-fail-relay', 'https://relay.example.com/v1'),
    ).rejects.toThrow(/remote models failed/);
    await expect(
      port.listRemoteOpenAiModelsForProvider(
        'p-saved',
        'https://mock-models.example.com',
      ),
    ).resolves.toEqual(['mock-gpt-4', 'mock-gpt-4o-mini']);
  });
});

describe('mock detectApiEndpointTypes', () => {
  it('returns canned supported endpoint types without making a network request', async () => {
    const port = createMockProviderPort();
    await expect(port.detectApiEndpointTypes('https://claude.example.com', 'sk-test')).resolves.toEqual([
      'messages',
    ]);
    await expect(port.detectApiEndpointTypes('https://grok.example.com', 'sk-test')).resolves.toEqual([
      'responses',
      'chat_completions',
    ]);
  });
});
