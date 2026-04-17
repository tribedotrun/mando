import React, { useState, useCallback, useRef } from 'react';
import { useViewKeyHandler } from '#renderer/global/runtime/useKeyboardShortcuts';
import { MetricsRow } from '#renderer/domains/captain/ui/MetricsRow';
import { PipelineStats } from '#renderer/domains/captain/ui/PipelineStats';
import { ActivityStrip } from '#renderer/domains/captain/ui/ActivityStrip';
import {
  InlineTaskCreate,
  type InlineTaskCreateHandle,
} from '#renderer/domains/captain/ui/InlineTaskCreate';
import { ImageFeedbackModal } from '#renderer/global/ui/FeedbackModal';
import { useTaskNudge, useTaskHandoff } from '#renderer/domains/captain/runtime/hooks';
import type { WorkerDetail } from '#renderer/global/types';

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
          onStopWorker={async (worker) => {
            await handoffMut.mutateAsync({ id: worker.id });
          }}
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
        <ImageFeedbackModal
          testId="nudge-modal"
          title="Nudge worker"
          subtitle={nudgeWorker.title}
          placeholder="Nudge message"
          initialValue="Keep going. Ship the next concrete step."
          buttonLabel="Nudge"
          pendingLabel="Nudging..."
          isPending={nudgeMut.isPending}
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
