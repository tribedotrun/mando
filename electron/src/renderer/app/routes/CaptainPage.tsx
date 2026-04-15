import React, { useRef } from 'react';
import { CaptainView } from '#renderer/domains/captain/components/CaptainView';
import type { InlineTaskCreateHandle } from '#renderer/domains/captain/components/InlineTaskCreate';
import { ErrorBoundary } from '#renderer/global/components/ErrorBoundary';
import { useMountEffect } from '#renderer/global/hooks/useMountEffect';
import { useUIStore } from '#renderer/app/uiStore';

export function CaptainPage(): React.ReactElement {
  const inlineRef = useRef<InlineTaskCreateHandle>(null);

  useMountEffect(() => {
    useUIStore.getState().registerInlineFocus(() => inlineRef.current?.focus());
    return () => useUIStore.getState().unregisterInlineFocus();
  });

  return (
    <div className="absolute inset-0 overflow-auto bg-background px-8 pb-6">
      <ErrorBoundary fallbackLabel="Captain view">
        <CaptainView active inlineRef={inlineRef} />
      </ErrorBoundary>
    </div>
  );
}
