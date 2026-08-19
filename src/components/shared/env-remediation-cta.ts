export function envOneClickInstallVariant(pageHasPrimaryCta: boolean): 'default' | 'secondary' {
  return pageHasPrimaryCta ? 'secondary' : 'default';
}
