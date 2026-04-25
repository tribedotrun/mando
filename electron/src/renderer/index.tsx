import React from 'react';
import { createRoot } from 'react-dom/client';
import { RouterProvider } from '@tanstack/react-router';
import { DataProvider } from '#renderer/app/DataProvider';
import { router } from '#renderer/app/router';
import { ErrorBoundary } from '#renderer/global/ui/ErrorBoundary';
import { Toaster } from '#renderer/global/ui/primitives/sonner';
import { TooltipProvider } from '#renderer/global/ui/primitives/tooltip';
import log from '#renderer/global/service/logger';
import './index.css';

// PR #883 invariant #1: capture renderer-side unhandled errors and
// promise rejections before React StrictMode or the ErrorBoundary get a
// chance to consume or retry them. These are the browser equivalents of
// `process.on('uncaughtException' / 'unhandledRejection')`.
window.addEventListener('unhandledrejection', (event) => {
  log.error('renderer unhandledrejection', event.reason);
});
window.addEventListener('error', (event) => {
  log.error('renderer uncaught error', event.error ?? event.message);
});

const container = document.getElementById('root');
if (!container) throw new Error('Root element not found');

const root = createRoot(container);
root.render(
  <React.StrictMode>
    <ErrorBoundary>
      <TooltipProvider disableHoverableContent>
        <Toaster />
        <DataProvider>
          <RouterProvider router={router} />
        </DataProvider>
      </TooltipProvider>
    </ErrorBoundary>
  </React.StrictMode>,
);
