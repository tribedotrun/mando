import React from 'react';
import { X } from 'lucide-react';
import {
  useBulkCreateStore,
  type BulkCreatePhase,
} from '#renderer/domains/captain/stores/bulkCreateStore';
import { Button } from '#renderer/components/ui/button';
import { Spinner } from '#renderer/global/components/Spinner';

function progressText(phase: BulkCreatePhase): string {
  switch (phase.step) {
    case 'parsing':
      return 'Parsing tasks...';
    case 'creating':
      return `Adding ${phase.done}/${phase.total}...`;
    case 'done':
      return `Added ${phase.count} task${phase.count === 1 ? '' : 's'}`;
    case 'error':
      return phase.message;
    case 'idle':
      return '';
  }
}

export function BulkCreateProgress(): React.ReactElement | null {
  const phase = useBulkCreateStore((s) => s.phase);
  const dismiss = useBulkCreateStore((s) => s.dismiss);

  if (phase.step === 'idle') return null;

  const isActive = phase.step === 'parsing' || phase.step === 'creating';
  const isError = phase.step === 'error';

  return (
    <>
      <style>{`
        @keyframes bulk-in {
          from { opacity: 0; transform: translateY(8px); }
          to   { opacity: 1; transform: translateY(0); }
        }
      `}</style>
      <div
        className="fixed bottom-4 left-4 z-[350] flex items-center gap-2 rounded-lg bg-muted px-4 py-2 shadow-xl"
        style={{ animation: 'bulk-in 200ms ease-out' }}
      >
        {isActive && <Spinner />}

        <span
          className={`text-[13px] font-medium ${isError ? 'text-destructive' : 'text-foreground'}`}
        >
          {progressText(phase)}
        </span>

        {!isActive && (
          <Button
            variant="ghost"
            size="icon-xs"
            onClick={dismiss}
            aria-label="Dismiss"
            className="ml-0.5"
          >
            <X size={14} />
          </Button>
        )}
      </div>
    </>
  );
}
