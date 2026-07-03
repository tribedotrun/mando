import { useState } from 'react';
import {
  useCodexCredentialAdd,
  useCredentialsList,
  useCredentialRemove,
} from '#renderer/domains/settings/runtime/hooks';

export function useSettingsAccounts() {
  const [showTokenInput, setShowTokenInput] = useState(false);
  const [showCodexInput, setShowCodexInput] = useState(false);

  const { data, isLoading } = useCredentialsList();
  const removeMut = useCredentialRemove();
  const codexAddMut = useCodexCredentialAdd();

  const all = data?.credentials ?? [];
  const claudeItems = all.filter((c) => c.provider === 'claude');
  const codexItems = all.filter((c) => c.provider === 'codex');

  return {
    visibility: {
      showTokenInput,
      setShowTokenInput,
      showCodexInput,
      setShowCodexInput,
    },
    claude: { items: claudeItems, isLoading },
    codex: { items: codexItems, isLoading },
    mutations: { removeMut, codexAddMut },
    /** Back-compat: existing UI reads `.credentials.items`. */
    credentials: { items: all, isLoading },
  };
}
