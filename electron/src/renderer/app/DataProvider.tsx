import React, { useCallback, useState } from 'react';
import { QueryClientProvider } from '@tanstack/react-query';
import { queryClient } from '#renderer/global/providers/queryClient';
import { DataProviderInner } from '#renderer/app/DataProviderInner';

export function DataProvider({ children }: { children: React.ReactNode }): React.ReactElement {
  const [dataPlaneEpoch, setDataPlaneEpoch] = useState(0);

  const resetDataPlane = useCallback(() => {
    queryClient.clear();
    setDataPlaneEpoch((prev) => prev + 1);
  }, []);

  return (
    <QueryClientProvider client={queryClient}>
      <DataProviderInner key={dataPlaneEpoch} resetDataPlane={resetDataPlane}>
        {children}
      </DataProviderInner>
    </QueryClientProvider>
  );
}
