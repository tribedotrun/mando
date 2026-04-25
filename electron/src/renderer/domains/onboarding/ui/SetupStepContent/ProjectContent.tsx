import React from 'react';
import { useAddProjectFromPicker } from '#renderer/domains/onboarding/runtime/useAddProjectFromPicker';
import { Button } from '#renderer/global/ui/primitives/button';

export function ProjectContent(): React.ReactElement {
  const { pickAndAdd, adding } = useAddProjectFromPicker();

  return (
    <div>
      <Button size="xs" onClick={() => void pickAndAdd()} disabled={adding}>
        {adding ? 'Adding…' : 'Choose folder'}
      </Button>
    </div>
  );
}
