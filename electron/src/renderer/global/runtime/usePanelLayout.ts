import { useDefaultLayout } from 'react-resizable-panels';

/**
 * Wraps useDefaultLayout with localStorage so UI components
 * never reference the storage mechanism directly.
 */
export function usePanelLayout(id: string) {
  return useDefaultLayout({ id, storage: localStorage });
}
