import { useState } from 'react';
import { useCredentialsList, useCredentialRemove } from '#renderer/domains/settings/runtime/hooks';

export function useSettingsAccounts() {
  const [showTokenInput, setShowTokenInput] = useState(false);

  const { data, isLoading } = useCredentialsList();
  const removeMut = useCredentialRemove();

  const credentials = data?.credentials ?? [];

  return {
    visibility: { showTokenInput, setShowTokenInput },
    credentials: { items: credentials, isLoading },
    mutations: { removeMut },
  };
}
