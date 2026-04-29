import React, { useCallback } from 'react';
import type { ItemStatus, TimelineEvent } from '#renderer/global/types';
import { ClarifierFailedBlock } from '#renderer/domains/captain/ui/ClarifierFailedCard/ClarifierFailedBlock';
import type { ClarifierFailedPayload } from '#renderer/domains/captain/ui/ClarifierFailedCard/types';
import log from '#renderer/global/service/logger';

export function ClarifierFailedRow({
  taskStatus,
  event,
  payload,
}: {
  taskStatus: ItemStatus;
  event: TimelineEvent;
  payload: ClarifierFailedPayload;
}): React.ReactElement {
  const onReanswer = useCallback(() => {
    const textarea = document.querySelector<HTMLTextAreaElement>(
      '[data-clarifier-target="answer"]',
    );
    if (!textarea) {
      log.warn('clarifier re-answer target not found in DOM (form likely hidden)');
      return;
    }
    textarea.scrollIntoView({ behavior: 'smooth', block: 'center' });
    textarea.focus({ preventScroll: true });
  }, []);
  return (
    <ClarifierFailedBlock
      event={event}
      apiErrorStatus={payload.api_error_status}
      message={payload.message}
      onReanswer={taskStatus === 'needs-clarification' ? onReanswer : undefined}
    />
  );
}
