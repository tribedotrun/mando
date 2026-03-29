import React from 'react';
import { createRoot } from 'react-dom/client';
import { DataProvider } from '#renderer/DataProvider';
import { App } from '#renderer/App';
import { VoiceInput } from '#renderer/components/VoiceInput';
import { ErrorBoundary } from '#renderer/components/ErrorBoundary';
import './index.css';

const container = document.getElementById('root');
if (!container) throw new Error('Root element not found');

const isVoiceMode = new URLSearchParams(window.location.search).has('voice');

const root = createRoot(container);
root.render(
  <React.StrictMode>
    <ErrorBoundary>
      {isVoiceMode ? (
        <VoiceInput />
      ) : (
        <DataProvider>
          <App />
        </DataProvider>
      )}
    </ErrorBoundary>
  </React.StrictMode>,
);
