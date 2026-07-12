import { useState } from 'react';
import { useCodexCredentialUpdateAuth } from '#renderer/domains/settings/runtime/useFeedbackUpdateCredentialAuth';

export function useUpdateCodexCredentialAuthForm(credentialId: number, onClose: () => void) {
  const [authJson, setAuthJson] = useState('');
  const updateMut = useCodexCredentialUpdateAuth();

  const handleClose = () => {
    setAuthJson('');
    onClose();
  };

  const handleUpdate = async () => {
    try {
      await updateMut.mutateAsync({ id: credentialId, authJson: authJson.trim() });
      setAuthJson('');
      onClose();
    } catch {
      // Toast surfaced by useFeedbackUpdateCredentialAuth; leave the field
      // intact for the retry.
    }
  };

  return {
    fields: { authJson, setAuthJson },
    state: { pending: updateMut.isPending },
    actions: { handleUpdate, handleClose },
  };
}
