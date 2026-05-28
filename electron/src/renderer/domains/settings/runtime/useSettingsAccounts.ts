import { useState } from 'react';
import { useCredentialsList, useCredentialRemove } from '#renderer/domains/settings/runtime/hooks';

export function useSettingsAccounts() {
  const [showTokenInput, setShowTokenInput] = useState(false);

  const { data, isLoading } = useCredentialsList();
  const removeMut = useCredentialRemove();

  const all = data?.credentials ?? [];

  return {
    visibility: {
      showTokenInput,
      setShowTokenInput,
    },
    claude: { items: all, isLoading },
    mutations: { removeMut },
    /** Back-compat: existing UI reads `.credentials.items`. */
    credentials: { items: all, isLoading },
  };
}
