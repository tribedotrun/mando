export const TAB_ROUTES: Record<string, string> = {
  captain: '/',
  scout: '/scout',
  sessions: '/sessions',
};

/**
 * Settings entry points must replace the history entry when the user is already
 * inside settings. Otherwise each shortcut or palette action pushes a new entry
 * and a single "Back to app" only steps to the previous settings section.
 */
export function isSettingsPath(pathname: string): boolean {
  return pathname.startsWith('/settings');
}

export function getPageTitle(pathname: string): string {
  if (pathname === '/' || pathname === '') return 'Tasks';
  if (pathname.startsWith('/scout')) return 'Scout';
  if (pathname.startsWith('/sessions')) return 'Sessions';
  if (pathname.startsWith('/settings')) return 'Settings';
  return '';
}
