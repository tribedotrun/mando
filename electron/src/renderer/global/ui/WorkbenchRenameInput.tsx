import React from 'react';
import { Input } from '#renderer/global/ui/input';
import { useWorkbenchRenameInput } from '#renderer/global/runtime/useWorkbenchRenameInput';

interface RenameInputProps {
  initialValue: string;
  onCommit: (value: string) => void;
  onCancel: () => void;
  className?: string;
}

export function WorkbenchRenameInput({
  initialValue,
  onCommit,
  onCancel,
  className,
}: RenameInputProps): React.ReactElement {
  const { value, setValue, inputRefCb, commit, cancel } = useWorkbenchRenameInput(
    initialValue,
    onCommit,
    onCancel,
  );
  return (
    <Input
      ref={inputRefCb}
      value={value}
      onChange={(event) => setValue(event.target.value)}
      onKeyDown={(event) => {
        if (event.key === 'Enter') commit();
        if (event.key === 'Escape') cancel();
      }}
      onBlur={commit}
      className={
        className ?? 'h-6 w-full rounded border-ring bg-secondary px-2 text-[12px] font-normal'
      }
    />
  );
}
