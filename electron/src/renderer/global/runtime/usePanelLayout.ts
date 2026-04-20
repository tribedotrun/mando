import { useDefaultLayout } from 'react-resizable-panels';
import { createPrefixedStorage } from '#renderer/global/providers/persistence';

const panelStorage = createPrefixedStorage('', 'global/runtime/usePanelLayout');

/**
 * Wraps useDefaultLayout with the typed persistence boundary so UI
 * components never reference the storage mechanism directly.
 */
export function usePanelLayout(id: string) {
  return useDefaultLayout({ id, storage: panelStorage });
}
