export const TAB_ROUTES: Record<string, string> = {
  captain: '/',
  scout: '/scout',
  sessions: '/sessions',
};

export function getPageTitle(pathname: string): string {
  if (pathname === '/' || pathname === '') return 'Tasks';
  if (pathname.startsWith('/scout')) return 'Scout';
  if (pathname.startsWith('/sessions')) return 'Sessions';
  if (pathname.startsWith('/settings')) return 'Settings';
  return '';
}
