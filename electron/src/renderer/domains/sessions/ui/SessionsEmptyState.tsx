import React from 'react';
import { Monitor } from 'lucide-react';

export function SessionsEmptyState(): React.ReactElement {
  return (
    <div className="flex flex-col items-center justify-center py-16">
      <Monitor size={48} color="var(--text-4)" strokeWidth={1} className="mb-4" />
      <span className="text-subheading text-muted-foreground">No sessions yet</span>
    </div>
  );
}
