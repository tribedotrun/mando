// Type augmentations for the renderer. Keeps callers compliant with
// @typescript-eslint/consistent-type-assertions by removing the need for
// `as React.CSSProperties` on Electron-specific style properties.

import 'react';

declare module 'react' {
  interface CSSProperties {
    /** Electron drag-region for native title bar regions. */
    WebkitAppRegion?: 'drag' | 'no-drag';
    /** Sonner toast theming variables. */
    '--normal-bg'?: string;
    '--normal-text'?: string;
    '--normal-border'?: string;
    '--border-radius'?: string;
    '--gray1'?: string;
    '--gray2'?: string;
    '--gray5'?: string;
    '--gray12'?: string;
  }
}
