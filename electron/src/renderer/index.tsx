import React from 'react';
import { createRoot } from 'react-dom/client';
import { DataProvider } from '#renderer/app/DataProvider';
import { App } from '#renderer/app/App';
import { ErrorBoundary } from '#renderer/global/components/ErrorBoundary';
import { Toaster } from '#renderer/global/components/Toaster';
import './index.css';

const container = document.getElementById('root');
if (!container) throw new Error('Root element not found');

const root = createRoot(container);
root.render(
  <React.StrictMode>
    <ErrorBoundary>
      <Toaster />
      <DataProvider>
        <App />
      </DataProvider>
    </ErrorBoundary>
  </React.StrictMode>,
);
