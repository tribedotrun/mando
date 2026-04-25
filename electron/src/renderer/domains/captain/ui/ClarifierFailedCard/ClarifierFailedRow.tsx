import React from 'react';
import { useClarifierRetry } from '#renderer/domains/captain/runtime/useClarifierRetry';
import type { TimelineEvent } from '#renderer/global/types';
import { ClarifierFailedBlock } from '#renderer/domains/captain/ui/ClarifierFailedCard/ClarifierFailedBlock';
import type { ClarifierFailedPayload } from '#renderer/domains/captain/ui/ClarifierFailedCard/types';

export function ClarifierFailedRow({
  taskId,
  event,
  payload,
}: {
  taskId: number;
  event: TimelineEvent;
  payload: ClarifierFailedPayload;
}): React.ReactElement {
  const onRetry = useClarifierRetry(taskId);
  return (
    <ClarifierFailedBlock
      event={event}
      apiErrorStatus={payload.api_error_status}
      message={payload.message}
      onRetry={onRetry}
    />
  );
}
