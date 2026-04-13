import {
  createRootRoute,
  createRoute,
  createRouter,
  createMemoryHistory,
} from '@tanstack/react-router';
import { z } from 'zod';
import { RootShell } from '#renderer/app/routes/RootShell';
import { AppLayout } from '#renderer/app/routes/AppLayout';
import { CaptainPage } from '#renderer/app/routes/CaptainPage';
import { TaskDetailPage } from '#renderer/app/routes/TaskDetailPage';
import { ScoutPage } from '#renderer/app/routes/ScoutPage';
import { SessionsPage } from '#renderer/app/routes/SessionsPage';
import { TerminalPageRoute } from '#renderer/app/routes/TerminalPageRoute';
import { SettingsPageRoute } from '#renderer/app/routes/SettingsPageRoute';
import { TranscriptPage } from '#renderer/app/routes/TranscriptPage';
import log from '#renderer/logger';

// ---------------------------------------------------------------------------
// Route tree
// ---------------------------------------------------------------------------

const rootRoute = createRootRoute({ component: RootShell });

// Pathless layout: sidebar + content. Wraps all routes except settings.
const appLayout = createRoute({
  getParentRoute: () => rootRoute,
  id: '_app',
  component: AppLayout,
});

// -- Captain (task list) --
const captainRoute = createRoute({
  getParentRoute: () => appLayout,
  path: '/captain',
  validateSearch: z.object({
    project: z.string().optional().catch(undefined),
  }),
  component: CaptainPage,
});

// -- Task detail --
const taskDetailRoute = createRoute({
  getParentRoute: () => appLayout,
  path: '/captain/tasks/$taskId',
  validateSearch: z.object({
    tab: z.string().optional().catch(undefined),
  }),
  component: TaskDetailPage,
});

// -- Scout --
const scoutRoute = createRoute({
  getParentRoute: () => appLayout,
  path: '/scout',
  validateSearch: z.object({
    item: z.coerce.number().int().positive().optional().catch(undefined),
  }),
  component: ScoutPage,
});

// -- Sessions --
const sessionsRoute = createRoute({
  getParentRoute: () => appLayout,
  path: '/sessions',
  component: SessionsPage,
});

// -- Transcript detail --
const transcriptRoute = createRoute({
  getParentRoute: () => appLayout,
  path: '/sessions/$sessionId',
  validateSearch: z.object({
    caller: z.string().optional().catch(undefined),
    cwd: z.string().optional().catch(undefined),
    project: z.string().optional().catch(undefined),
    taskTitle: z.string().optional().catch(undefined),
  }),
  component: TranscriptPage,
});

// -- Terminal --
const terminalRoute = createRoute({
  getParentRoute: () => appLayout,
  path: '/terminal',
  validateSearch: z.object({
    project: z.string().catch(''),
    cwd: z.string().optional().catch(undefined),
    resume: z.string().optional().catch(undefined),
    name: z.string().optional().catch(undefined),
  }),
  beforeLoad: ({ search, navigate }) => {
    if (!(search as { project: string }).project) {
      log.warn('[terminal-route] beforeLoad: empty project, redirecting', search);
      throw navigate({ to: '/captain', replace: true });
    }
  },
  component: TerminalPageRoute,
});

// -- Settings (own layout, no sidebar) --
const settingsRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: '/settings/$section',
  component: SettingsPageRoute,
});

// -- Catch-all redirect to captain --
const indexRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: '/',
  beforeLoad: ({ navigate }) => {
    throw navigate({ to: '/captain' });
  },
});

// ---------------------------------------------------------------------------
// Route tree assembly
// ---------------------------------------------------------------------------

const routeTree = rootRoute.addChildren([
  indexRoute,
  appLayout.addChildren([
    captainRoute,
    taskDetailRoute,
    scoutRoute,
    sessionsRoute,
    transcriptRoute,
    terminalRoute,
  ]),
  settingsRoute,
]);

// ---------------------------------------------------------------------------
// Router instance
// ---------------------------------------------------------------------------

const memoryHistory = createMemoryHistory({ initialEntries: ['/captain'] });

export const router = createRouter({
  routeTree,
  history: memoryHistory,
  defaultPreload: false,
});

// Type registration for type-safe navigation
declare module '@tanstack/react-router' {
  interface Register {
    router: typeof router;
  }
}
