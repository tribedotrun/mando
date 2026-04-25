import { useMatchRoute } from '@tanstack/react-router';
import type { Tab } from '#renderer/global/runtime/SidebarContext';

export function useSidebarActiveTab(): Tab {
  const matchRoute = useMatchRoute();

  if (matchRoute({ to: '/scout', fuzzy: true })) {
    return 'scout';
  }

  if (matchRoute({ to: '/sessions', fuzzy: true })) {
    return 'sessions';
  }

  return 'captain';
}
