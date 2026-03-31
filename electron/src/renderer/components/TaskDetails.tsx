import React from 'react';

export function TaskEmptyState(): React.ReactElement {
  return (
    <div className="flex flex-col items-center justify-center py-16">
      <svg width="48" height="48" viewBox="0 0 48 48" fill="none" className="mb-4">
        <rect
          x="8"
          y="8"
          width="32"
          height="32"
          rx="6"
          stroke="var(--color-text-4)"
          strokeWidth="1.5"
        />
        <path
          d="M18 24l4 4 8-8"
          stroke="var(--color-text-4)"
          strokeWidth="1.5"
          strokeLinecap="round"
          strokeLinejoin="round"
        />
      </svg>
      <span className="text-subheading mb-1" style={{ color: 'var(--color-text-2)' }}>
        No tasks yet
      </span>
      <span className="text-body" style={{ color: 'var(--color-text-3)' }}>
        Create a task and Captain will pick it up automatically.
      </span>
    </div>
  );
}
