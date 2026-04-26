import React, { useCallback, useRef } from 'react';
import { useViewKeyHandler } from '#renderer/global/runtime/useKeyboardShortcuts';
import { PipelineStats } from '#renderer/domains/captain/ui/PipelineStats';
import { ActivityStrip } from '#renderer/domains/captain/ui/ActivityStrip';
import {
  InlineTaskCreate,
  type InlineTaskCreateHandle,
} from '#renderer/domains/captain/ui/InlineTaskCreate';

interface Props {
  active?: boolean;
  inlineRef?: React.RefObject<InlineTaskCreateHandle | null>;
}

export function CaptainView({ active = true, inlineRef: externalRef }: Props): React.ReactElement {
  const ownRef = useRef<InlineTaskCreateHandle>(null);
  const inlineRef = externalRef ?? ownRef;

  const handleKey = useCallback(
    (key: string, e: KeyboardEvent) => {
      if (key === 'c') {
        e.preventDefault();
        inlineRef.current?.focus();
      }
    },
    [inlineRef],
  );

  useViewKeyHandler(handleKey, active);

  return (
    <div className="flex h-full flex-col items-center pt-8">
      {/* Pipeline stats + activity heatmap */}
      <PipelineStats />
      <ActivityStrip />

      {/* Spacer pushes the input toward the center */}
      <div className="flex-1" />

      {/* Inline task creation */}
      <div className="w-full max-w-[640px] pb-8">
        <InlineTaskCreate ref={inlineRef} />
      </div>

      {/* Spacer below for balance */}
      <div className="flex-[0.6]" />
    </div>
  );
}
