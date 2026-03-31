import React, { useState } from 'react';
import { useQuery, useQueryClient } from '@tanstack/react-query';
import { approveKnowledgeLesson, fetchKnowledgePending } from '#renderer/api';
import { useToastStore } from '#renderer/stores/toastStore';
import { getErrorMessage } from '#renderer/utils';

export function KnowledgeReviewCard(): React.ReactElement {
  const queryClient = useQueryClient();
  const [approvingId, setApprovingId] = useState<string | null>(null);
  const { data, isLoading, error } = useQuery({
    queryKey: ['knowledge', 'pending'],
    queryFn: fetchKnowledgePending,
    refetchInterval: 30_000,
  });

  const pending = data?.pending ?? [];

  const handleApprove = async (id: string) => {
    setApprovingId(id);
    try {
      await approveKnowledgeLesson({ id });
      await queryClient.invalidateQueries({ queryKey: ['knowledge', 'pending'] });
      useToastStore.getState().add('success', 'Knowledge lesson approved');
    } catch (err) {
      useToastStore.getState().add('error', getErrorMessage(err, 'Approve failed'));
    } finally {
      setApprovingId(null);
    }
  };

  return (
    <div
      className="rounded-lg border p-4"
      style={{
        borderColor: 'var(--color-border)',
        background: 'var(--color-surface-1)',
      }}
    >
      <div className="mb-2 flex items-center justify-between">
        <h3 className="text-sm font-semibold" style={{ color: 'var(--color-text-2)' }}>
          Knowledge review
        </h3>
        <span className="text-[11px]" style={{ color: 'var(--color-text-4)' }}>
          {pending.length} pending
        </span>
      </div>

      {isLoading ? (
        <div className="text-xs" style={{ color: 'var(--color-text-4)' }}>
          Loading…
        </div>
      ) : error ? (
        <div className="text-xs" style={{ color: 'var(--color-error)' }}>
          {getErrorMessage(error, 'Failed to load knowledge')}
        </div>
      ) : pending.length === 0 ? (
        <div className="text-xs" style={{ color: 'var(--color-text-4)' }}>
          No pending lessons.
        </div>
      ) : (
        <div className="space-y-2">
          {pending.slice(0, 5).map((lesson) => (
            <div
              key={lesson.id}
              className="flex items-start justify-between gap-3 rounded border px-3 py-2"
              style={{
                borderColor: 'var(--color-border-subtle)',
                background: 'var(--color-surface-2)',
              }}
            >
              <div className="min-w-0 flex-1">
                <div className="text-xs font-medium" style={{ color: 'var(--color-text-2)' }}>
                  {lesson.title || lesson.id}
                </div>
                <div className="text-[11px]" style={{ color: 'var(--color-text-4)' }}>
                  {lesson.source || 'unknown source'}
                </div>
              </div>
              <button
                onClick={() => handleApprove(lesson.id)}
                disabled={approvingId === lesson.id}
                className="rounded px-2 py-1 text-[11px] font-medium disabled:opacity-50"
                style={{
                  background: 'var(--color-success-bg)',
                  color: 'var(--color-success)',
                  border: 'none',
                  cursor: 'pointer',
                }}
              >
                {approvingId === lesson.id ? 'Approving…' : 'Approve'}
              </button>
            </div>
          ))}
        </div>
      )}
    </div>
  );
}
