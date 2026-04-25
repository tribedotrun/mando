import React from 'react';
import type { TimelineEvent } from '#renderer/global/types';
import { PlanSummaryBlock } from '#renderer/domains/captain/ui/PlanCompletedBlock/PlanSummaryBlock';

export function CompletedPlanBlock({ event }: { event: TimelineEvent }): React.ReactElement {
  return <PlanSummaryBlock event={event} status="completed-no-pr" title="Planning complete" />;
}
