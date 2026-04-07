import React from 'react';
import { Toaster as SonnerToaster } from 'sonner';

export function Toaster(): React.ReactElement {
  return (
    <SonnerToaster
      position="bottom-right"
      toastOptions={{
        style: {
          background: 'var(--color-surface-2)',
          border: '1px solid var(--color-border-subtle)',
          color: 'var(--color-text-1)',
          borderRadius: 8,
          fontSize: 14,
          fontFamily: 'var(--font-sans)',
        },
      }}
      theme="dark"
    />
  );
}
