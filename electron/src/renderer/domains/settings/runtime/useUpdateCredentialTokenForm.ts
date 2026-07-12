import { useState } from 'react';
import { useCredentialUpdateToken } from '#renderer/domains/settings/runtime/useFeedbackUpdateCredentialAuth';

export function useUpdateCredentialTokenForm(credentialId: number, onClose: () => void) {
  const [token, setToken] = useState('');
  const updateMut = useCredentialUpdateToken();

  const handleClose = () => {
    setToken('');
    onClose();
  };

  const handleUpdate = async () => {
    try {
      await updateMut.mutateAsync({ id: credentialId, token: token.trim() });
      setToken('');
      onClose();
    } catch {
      // Toast surfaced by useFeedbackUpdateCredentialAuth; leave the field
      // intact for the retry.
    }
  };

  return {
    fields: { token, setToken },
    state: { pending: updateMut.isPending },
    actions: { handleUpdate, handleClose },
  };
}
