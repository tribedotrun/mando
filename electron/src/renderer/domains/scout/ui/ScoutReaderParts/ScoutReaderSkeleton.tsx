import React from 'react';
import { Skeleton } from '#renderer/global/ui/primitives/skeleton';

export function ScoutReaderSkeleton(): React.ReactElement {
  return (
    <div data-testid="scout-reader" className="h-full px-5 py-4">
      <div className="mx-auto max-w-[720px] space-y-4 py-8">
        <Skeleton className="h-6 w-48" />
        <Skeleton className="h-4 w-32" />
        <Skeleton className="h-4 w-full" />
        <Skeleton className="h-4 w-3/4" />
        <Skeleton className="h-4 w-2/3" />
      </div>
    </div>
  );
}
