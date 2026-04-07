import React from 'react';
import { createRoot } from 'react-dom/client';
import { DataProvider } from '#renderer/app/DataProvider';
import { App } from '#renderer/app/App';
import { ErrorBoundary } from '#renderer/global/components/ErrorBoundary';
import { Toaster } from '#renderer/components/ui/sonner';
import { TooltipProvider } from '#renderer/components/ui/tooltip';
import './index.css';

const container = document.getElementById('root');
if (!container) throw new Error('Root element not found');

const root = createRoot(container);
root.render(
  <React.StrictMode>
    <ErrorBoundary>
      <TooltipProvider>
        <Toaster />
        <DataProvider>
          <App />
        </DataProvider>
      </TooltipProvider>
    </ErrorBoundary>
  </React.StrictMode>,
);
