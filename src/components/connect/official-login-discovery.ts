import type { OfficialLoginDiscovery } from '@/components/login-kernel';
import { listOAuthOptions, oauthSupported } from '@/lib/api/account';
import { waitOfficialLogin } from '@/lib/api/official-login';

/** Shared option probe and wait. Pages still own start / finish / cancel. */
export const officialLoginDiscovery: OfficialLoginDiscovery = {
  supported: (agentId) => oauthSupported(agentId),
  listOptions: (agentId) => listOAuthOptions(agentId),
  wait: (session, isCurrent) => waitOfficialLogin(session, isCurrent),
};
