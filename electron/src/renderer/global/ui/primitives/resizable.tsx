import * as React from 'react';
import { Group, Panel, Separator } from 'react-resizable-panels';

import { cn } from '#renderer/global/service/cn';

function ResizablePanelGroup({ className, ...props }: React.ComponentProps<typeof Group>) {
  return (
    <Group
      data-slot="resizable-panel-group"
      className={cn('flex h-full w-full', className)}
      {...props}
    />
  );
}

function ResizablePanel(props: React.ComponentProps<typeof Panel>) {
  return <Panel data-slot="resizable-panel" {...props} />;
}

function ResizableHandle({ className, ...props }: React.ComponentProps<typeof Separator>) {
  return (
    <Separator
      data-slot="resizable-handle"
      className={cn(
        'bg-border-subtle relative flex w-px items-center justify-center after:absolute after:inset-y-0 after:-left-1 after:-right-1 hover:bg-surface-2 transition-colors',
        className,
      )}
      {...props}
    />
  );
}

export { ResizablePanelGroup, ResizablePanel, ResizableHandle };
