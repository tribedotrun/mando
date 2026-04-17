import { createContext, use } from 'react';
import type { SSEConnectionStatus } from '#renderer/global/types';

export interface DataContextValue {
  sseStatus: SSEConnectionStatus;
}

export const DataContext = createContext<DataContextValue>({
  sseStatus: 'disconnected',
});

export function useDataContext(): DataContextValue {
  return use(DataContext);
}
