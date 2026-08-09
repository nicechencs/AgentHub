/** Browser-safe host platform detection and runtime channel selection. */

export type HostPlatform = 'windows' | 'macos' | 'linux' | 'unknown';

export type RuntimeInstallChannel = 'winget' | 'brew';

export function detectHostPlatform(input?: {
  platform?: string;
  userAgent?: string;
}): HostPlatform {
  const platform = input?.platform ?? (typeof navigator !== 'undefined' ? navigator.platform : '');
  const userAgent = input?.userAgent ?? (typeof navigator !== 'undefined' ? navigator.userAgent : '');
  const value = `${platform} ${userAgent}`.toLowerCase();

  if (/windows|win32|win64|windows phone/.test(value)) return 'windows';
  if (/macintosh|mac os x|macintel|macppc|mac68k/.test(value)) return 'macos';
  if (/linux|x11|ubuntu|fedora|debian/.test(value)) return 'linux';
  return 'unknown';
}

export function getRuntimeInstallChannel(
  platform: HostPlatform = detectHostPlatform(),
): RuntimeInstallChannel {
  return platform === 'macos' ? 'brew' : 'winget';
}

export const runtimeInstallChannel = getRuntimeInstallChannel;
