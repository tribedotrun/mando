import { useState } from 'react';
import { useCodexCredentialAdd } from '#renderer/domains/settings/runtime/hooks';

export function useAddCodexCredentialForm(onClose: () => void) {
  const [authJson, setAuthJson] = useState('');
  const [label, setLabel] = useState('');
  const addMut = useCodexCredentialAdd();

  const reset = () => {
    setAuthJson('');
    setLabel('');
  };

  const handleClose = () => {
    reset();
    onClose();
  };

  const handleAdd = async () => {
    // mutateAsync + try/catch keeps form state on failure (so the user
    // can re-paste a fixed auth.json without retyping the label) and
    // avoids the per-call onSuccess orphaning hazard under React 18
    // Strict Mode remounts.
    try {
      await addMut.mutateAsync({ label: label.trim(), authJson: authJson.trim() });
      reset();
      onClose();
    } catch {
      // Toast surfaced by useFeedbackCodexCredentials; leave fields
      // intact for the retry.
    }
  };

  return {
    fields: { authJson, setAuthJson, label, setLabel },
    state: { pending: addMut.isPending },
    actions: { handleAdd, handleClose },
  };
}
