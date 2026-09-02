/** Live-vs-pool login notice emitted after `list_providers`. */

export const PROVIDER_BINDING_HEAL_EVENT = 'provider-binding-heal';

export type ProviderBindingHealKind = 'healed' | 'conflict';

export interface ProviderBindingHealPayload {
  kind: ProviderBindingHealKind;
  agent: string;
  fromId?: string | null;
  fromName?: string | null;
  toId?: string | null;
  toName?: string | null;
  liveHint?: string | null;
  messageKey?: string | null;
}
