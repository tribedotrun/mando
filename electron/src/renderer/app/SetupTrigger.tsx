import React from 'react';
import { ChevronUp } from 'lucide-react';
import { pct } from '#renderer/utils';
import { SetupChecklist } from '#renderer/domains/onboarding';
import { Button } from '#renderer/components/ui/button';

export interface SetupProgress {
  completed: number;
  total: number;
  currentStep: string;
}

interface Props {
  progress: SetupProgress;
  active: boolean;
  onToggle: () => void;
  onDismiss: () => void;
}

export function SetupTrigger({ progress, active, onToggle, onDismiss }: Props): React.ReactElement {
  const progressPct = pct(progress.completed, progress.total);

  return (
    <div className="relative mt-2">
      {active && (
        <div
          data-testid="setup-popover"
          className="absolute right-0 bottom-[calc(100%+6px)] left-0 z-[200] max-h-[420px] overflow-y-auto rounded-lg bg-muted shadow-[0_-4px_20px_rgba(0,0,0,0.5)]"
        >
          <SetupChecklist onDismiss={onDismiss} onMinimize={onToggle} />
        </div>
      )}

      <Button
        variant="ghost"
        data-testid="setup-trigger"
        onClick={onToggle}
        aria-label={`${active ? 'Hide' : 'Show'} setup checklist, ${progressPct}% complete`}
        aria-expanded={active}
        className={`flex h-auto w-full items-center gap-2 rounded-md px-2.5 py-2 transition-colors ${active ? 'bg-muted' : 'bg-transparent'}`}
      >
        <svg width="16" height="16" viewBox="0 0 20 20" className="shrink-0">
          <circle cx="10" cy="10" r="8" fill="none" stroke="var(--secondary)" strokeWidth="2" />
          <circle
            cx="10"
            cy="10"
            r="8"
            fill="none"
            stroke="var(--foreground)"
            strokeWidth="2"
            strokeDasharray={`${(progressPct / 100) * 50.3} 50.3`}
            strokeLinecap="round"
            transform="rotate(-90 10 10)"
          />
        </svg>
        <div className="flex flex-1 flex-col items-start">
          <span className="text-[12px] font-medium text-foreground">
            Get started <span className="font-normal text-text-3">{progressPct}%</span>
          </span>
          <span className="text-caption max-w-[120px] truncate text-text-3">
            {progress.currentStep}
          </span>
        </div>
        <ChevronUp
          size={12}
          color="var(--text-3)"
          strokeWidth={2.5}
          className={`shrink-0 transition-transform duration-150 ${active ? 'rotate-180' : ''}`}
        />
      </Button>
    </div>
  );
}
