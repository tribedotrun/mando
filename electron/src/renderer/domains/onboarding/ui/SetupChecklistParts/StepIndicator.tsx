import React from 'react';
import { Check } from 'lucide-react';

export function StepIndicator({ completed }: { completed: boolean }): React.ReactElement {
  if (completed) {
    return (
      <div className="flex size-4 shrink-0 items-center justify-center rounded-full bg-success">
        <Check size={9} color="var(--background)" strokeWidth={3} />
      </div>
    );
  }
  return <div className="size-4 shrink-0 rounded-full bg-muted-foreground/20" />;
}
