import { useState } from 'react';
import { useCredentialAdd } from '#renderer/domains/settings/runtime/hooks';

export function useAddCredentialForm(onClose: () => void) {
  const [token, setToken] = useState('');
  const [label, setLabel] = useState('');
  const addCredentialMut = useCredentialAdd();

  const reset = () => {
    setToken('');
    setLabel('');
  };

  const handleClose = () => {
    reset();
    onClose();
  };

  const handleAdd = () => {
    addCredentialMut.mutate(
      { label: label.trim(), token: token.trim() },
      {
        onSuccess: () => {
          reset();
          onClose();
        },
      },
    );
  };

  return {
    fields: { token, setToken, label, setLabel },
    state: { pending: addCredentialMut.isPending },
    actions: { handleAdd, handleClose },
  };
}
