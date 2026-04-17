import React from 'react';

interface EmptyStateProps {
  icon: React.ReactNode;
  heading: string;
  description: string;
  children?: React.ReactNode;
}

export function EmptyState({
  icon,
  heading,
  description,
  children,
}: EmptyStateProps): React.ReactElement {
  return (
    <div className="flex flex-col items-center justify-center py-16">
      <div className="mb-4">{icon}</div>
      <span className="text-subheading mb-1 text-muted-foreground">{heading}</span>
      <span className="text-body mb-4 text-text-3">{description}</span>
      {children}
    </div>
  );
}
