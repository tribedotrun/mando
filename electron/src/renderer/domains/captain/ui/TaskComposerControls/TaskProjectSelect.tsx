import React from 'react';
import { shortRepo } from '#renderer/global/service/utils';
import { Combobox } from '#renderer/global/ui/primitives/combobox';

interface TaskProjectSelectProps {
  projects: string[];
  value: string;
  onValueChange: (value: string) => void;
  testId: string;
}

export function TaskProjectSelect({
  projects,
  value,
  onValueChange,
  testId,
}: TaskProjectSelectProps): React.ReactElement | null {
  if (projects.length === 0) return null;

  return (
    <Combobox
      data-testid={testId}
      value={value}
      onValueChange={onValueChange}
      options={projects.map((item) => ({
        value: item,
        label: shortRepo(item),
      }))}
      placeholder="Project..."
      searchPlaceholder="Search projects..."
      emptyText="No projects found."
    />
  );
}
