import {
  createRootRoute,
  createRoute,
  createRouter,
  createMemoryHistory,
} from '@tanstack/react-router';
import { z } from 'zod';
import { RootFrame } from '#renderer/app/routes/RootFrame';
import { AppLayout } from '#renderer/app/routes/AppLayout';
import { CaptainPage } from '#renderer/app/routes/CaptainPage';
import { WorkbenchPage } from '#renderer/app/routes/WorkbenchPage';
import { ScoutPage } from '#renderer/app/routes/ScoutPage';
import { SessionsPage } from '#renderer/app/routes/SessionsPage';
import { SettingsPageRoute } from '#renderer/app/routes/SettingsPageRoute';
import { TranscriptPage } from '#renderer/app/routes/TranscriptPage';
import log from '#renderer/global/service/logger';

// ---------------------------------------------------------------------------
// Route tree
// ---------------------------------------------------------------------------

const rootRoute = createRootRoute({ component: RootFrame });

// Pathless layout: sidebar + content. Wraps all routes except settings.
const appLayout = createRoute({
  getParentRoute: () => rootRoute,
  id: '_app',
  component: AppLayout,
});

// -- Home (task list / dashboard) --
const homeRoute = createRoute({
  getParentRoute: () => appLayout,
  path: '/',
  validateSearch: z.object({
    project: z.string().optional().catch(undefined),
  }),
  component: CaptainPage,
});

// -- Workbench detail --
const workbenchRoute = createRoute({
  getParentRoute: () => appLayout,
  path: '/wb/$workbenchId',
  validateSearch: z.object({
    tab: z.string().optional().catch(undefined),
    resume: z.string().optional().catch(undefined),
    name: z.string().optional().catch(undefined),
    project: z.string().optional().catch(undefined),
  }),
  beforeLoad: ({ params }) => {
    if (params.workbenchId !== 'new' && Number.isNaN(Number(params.workbenchId))) {
      log.warn('[wb-route] invalid workbenchId', params.workbenchId);
    }
  },
  component: WorkbenchPage,
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

// -- Settings (own layout, no sidebar) --
const settingsRoute = createRoute({
  getParentRoute: () => rootRoute,
  path: '/settings/$section',
  component: SettingsPageRoute,
});

// ---------------------------------------------------------------------------
// Route tree assembly
// ---------------------------------------------------------------------------

const routeTree = rootRoute.addChildren([
  appLayout.addChildren([homeRoute, workbenchRoute, scoutRoute, sessionsRoute, transcriptRoute]),
  settingsRoute,
]);

// ---------------------------------------------------------------------------
// Router instance
// ---------------------------------------------------------------------------

const memoryHistory = createMemoryHistory({ initialEntries: ['/'] });

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
