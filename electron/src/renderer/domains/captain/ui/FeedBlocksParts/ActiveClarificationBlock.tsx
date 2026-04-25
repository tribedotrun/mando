import React from 'react';
import { ClarificationTab } from '#renderer/domains/captain/ui/ClarificationTab';
import type { ClarifierQuestion } from '#renderer/global/types';

export function ActiveClarificationBlock({
  taskId,
  questions,
}: {
  taskId: number;
  questions: ClarifierQuestion[];
}): React.ReactElement {
  return (
    <div className="mx-3 my-2">
      <ClarificationTab taskId={taskId} questions={questions} />
    </div>
  );
}
