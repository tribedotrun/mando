import React, { useState, useCallback, useRef } from 'react';
import { useViewKeyHandler } from '#renderer/global/hooks/useKeyboardShortcuts';
import { MetricsRow } from '#renderer/domains/captain/components/MetricsRow';
import { PipelineStats } from '#renderer/domains/captain/components/PipelineStats';
import { ActivityStrip } from '#renderer/domains/captain/components/ActivityStrip';
import {
  InlineTaskCreate,
  type InlineTaskCreateHandle,
} from '#renderer/domains/captain/components/InlineTaskCreate';
import { FeedbackModal } from '#renderer/domains/captain/components/FeedbackModal';
import { useTaskNudge, useTaskHandoff } from '#renderer/hooks/mutations';
import type { WorkerDetail } from '#renderer/types';

interface Props {
  active?: boolean;
  inlineRef?: React.RefObject<InlineTaskCreateHandle | null>;
}

export function CaptainView({ active = true, inlineRef: externalRef }: Props): React.ReactElement {
  const [nudgeWorker, setNudgeWorker] = useState<WorkerDetail | null>(null);
  const nudgeMut = useTaskNudge();
  const handoffMut = useTaskHandoff();
  const ownRef = useRef<InlineTaskCreateHandle>(null);
  const inlineRef = externalRef ?? ownRef;

  const handleKey = useCallback(
    (key: string, e: KeyboardEvent) => {
      if (nudgeWorker) return;
      if (key === 'c') {
        e.preventDefault();
        inlineRef.current?.focus();
      }
    },
    [nudgeWorker],
  );

  useViewKeyHandler(handleKey, active);

  return (
    <div className="flex h-full flex-col items-center pt-8">
      {/* Pipeline stats + activity heatmap */}
      <PipelineStats />
      <ActivityStrip />

      {/* Worker panel */}
      <div className="mt-4 w-full max-w-[640px]">
        <MetricsRow
          onNudge={setNudgeWorker}
          onStopWorker={(worker) =>
            handoffMut.mutateAsync({ id: worker.id }).then(
              () => undefined,
              () => undefined,
            )
          }
        />
      </div>

      {/* Spacer pushes the input toward the center */}
      <div className="flex-1" />

      {/* Inline task creation */}
      <div className="w-full max-w-[640px] pb-8">
        <InlineTaskCreate ref={inlineRef} />
      </div>

      {/* Spacer below for balance */}
      <div className="flex-[0.6]" />

      {/* Nudge modal */}
      {nudgeWorker && (
        <FeedbackModal
          testId="nudge-modal"
          title="Nudge worker"
          subtitle={nudgeWorker.title}
          placeholder="Nudge message"
          initialValue="Keep going. Ship the next concrete step."
          buttonLabel="Nudge"
          pendingLabel="Nudging..."
          isPending={nudgeMut.isPending}
          allowImages
          onSubmit={(msg, images) => {
            nudgeMut.mutate(
              { id: nudgeWorker.id, message: msg, images },
              { onSuccess: () => setNudgeWorker(null) },
            );
          }}
          onCancel={() => setNudgeWorker(null)}
        />
      )}
    </div>
  );
}
