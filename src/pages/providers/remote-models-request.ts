import { isLivePastedApiKey } from '@/lib/provider-detect';

export type RemoteModelsRequestDeps = {
  listRemoteOpenAiModels: (baseUrl: string, apiKey: string) => Promise<string[]>;
  listRemoteOpenAiModelsForProvider: (providerId: string, baseUrl: string) => Promise<string[]>;
};

export function requestRemoteModels(
  args: {
    baseUrl: string;
    apiKey: string;
    providerId?: string | null;
  },
  deps: RemoteModelsRequestDeps,
): Promise<string[]> {
  if (isLivePastedApiKey(args.apiKey)) {
    return deps.listRemoteOpenAiModels(args.baseUrl, args.apiKey);
  }
  if (args.providerId) {
    return deps.listRemoteOpenAiModelsForProvider(args.providerId, args.baseUrl);
  }
  return Promise.reject(new Error('no stored secret'));
}
