import React from 'react';
import { useWorkbenchRenameInput } from '#renderer/global/runtime/useWorkbenchRenameInput';
import { Input } from '#renderer/global/ui/primitives/input';

interface ProjectRenameInputProps {
  initialValue: string;
  onCommit: (value: string) => void;
  onCancel: () => void;
}

export function ProjectRenameInput({
  initialValue,
  onCommit,
  onCancel,
}: ProjectRenameInputProps): React.ReactElement {
  const { value, setValue, inputRefCb, commit, cancel } = useWorkbenchRenameInput(
    initialValue,
    onCommit,
    onCancel,
  );

  return (
    <div className="rounded-md px-1.5 py-1">
      <Input
        ref={inputRefCb}
        value={value}
        aria-label="Rename project"
        onChange={(e) => setValue(e.target.value)}
        onKeyDown={(e) => {
          if (e.key === 'Enter') commit();
          if (e.key === 'Escape') cancel();
        }}
        onBlur={commit}
        className="h-7 w-full rounded border-ring bg-secondary px-1.5 text-[13px] font-normal"
      />
    </div>
  );
}
