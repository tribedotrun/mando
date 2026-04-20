import { useState, useCallback, useRef } from 'react';

export function useWorkbenchRenameInput(
  initialValue: string,
  onCommit: (value: string) => void,
  onCancel: () => void,
) {
  const [value, setValue] = useState(initialValue);
  const submittedRef = useRef(false);

  const inputRefCb = useCallback((element: HTMLInputElement | null) => {
    if (element) {
      element.focus();
      element.select();
    }
  }, []);

  const commit = () => {
    if (submittedRef.current) return;
    submittedRef.current = true;
    onCommit(value);
  };

  const cancel = () => {
    if (submittedRef.current) return;
    submittedRef.current = true;
    onCancel();
  };

  return { value, setValue, inputRefCb, commit, cancel };
}
