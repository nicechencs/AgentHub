/**
 * OAuth flow identity helpers (production dialog lives in components/connect).
 * Kept here so existing OAuthFlowDialog.test.ts continues to import from shared.
 */
export {
  createOAuthFlowToken,
  isOAuthFlowTokenCurrent,
  openManualCallbackFallbackIfCurrent,
  type OAuthFlowToken,
} from '@/components/connect/OAuthFlowDialog';
