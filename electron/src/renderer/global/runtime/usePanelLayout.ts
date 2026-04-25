import { useDefaultLayout } from 'react-resizable-panels';
import { z } from 'zod';
import { createJsonStorage } from '#renderer/global/providers/persistence';

const panelLayoutSchema = z.union([
  z.record(z.string(), z.number()),
  z.record(z.string(), z.object({ layout: z.array(z.number()) }).passthrough()),
]);
const panelStorage = createJsonStorage('', panelLayoutSchema, 'global/runtime/usePanelLayout');

/**
 * Wraps useDefaultLayout with the typed persistence boundary so UI
 * components never reference the storage mechanism directly.
 */
export function usePanelLayout(id: string) {
  return useDefaultLayout({ id, storage: panelStorage });
}
