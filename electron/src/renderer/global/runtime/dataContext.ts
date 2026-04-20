import { createContext, use } from 'react';
import type { SSEConnectionStatus } from '#renderer/global/types';

export interface DataContextValue {
  sseStatus: SSEConnectionStatus;
  resetDataPlane: () => void;
}

export const DataContext = createContext<DataContextValue>({
  sseStatus: 'disconnected',
  resetDataPlane: () => {},
});

export function useDataContext(): DataContextValue {
  return use(DataContext);
}
