export type PortalNavigationItem = Readonly<{
  label: string;
  href: `/${string}` | '/';
}>;

export const portalNavigation = [
  { label: 'Organizations', href: '/organizations' },
  { label: 'Devices', href: '/devices' },
  { label: 'Billing', href: '/billing' },
  { label: 'Account', href: '/account' }
] as const satisfies readonly PortalNavigationItem[];

export function isCurrentPortalRoute(pathname: string, href: PortalNavigationItem['href']): boolean {
  return href === '/' ? pathname === href : pathname === href || pathname.startsWith(`${href}/`);
}
