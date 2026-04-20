import { useState } from 'react';
import {
  useCredentialsList,
  useCredentialAdd,
  useCredentialRemove,
} from '#renderer/domains/settings/runtime/hooks';

export function useSettingsAccounts() {
  const [setupToken, setSetupToken] = useState('');
  const [setupLabel, setSetupLabel] = useState('');
  const [showTokenInput, setShowTokenInput] = useState(false);

  const { data, isLoading } = useCredentialsList();
  const addTokenMut = useCredentialAdd();
  const removeMut = useCredentialRemove();

  const credentials = data?.credentials ?? [];

  const handleAddSuccess = () => {
    setSetupToken('');
    setSetupLabel('');
    setShowTokenInput(false);
  };

  const handleCancel = () => {
    setShowTokenInput(false);
    setSetupToken('');
    setSetupLabel('');
  };

  const handleAdd = () => {
    addTokenMut.mutate(
      { label: setupLabel.trim(), token: setupToken.trim() },
      { onSuccess: handleAddSuccess },
    );
  };

  return {
    setupToken,
    setSetupToken,
    setupLabel,
    setSetupLabel,
    showTokenInput,
    setShowTokenInput,
    credentials,
    isLoading,
    addTokenMut,
    removeMut,
    handleCancel,
    handleAdd,
  };
}
