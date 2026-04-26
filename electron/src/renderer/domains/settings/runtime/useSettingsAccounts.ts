import { useState } from 'react';
import {
  useCodexActiveCredential,
  useCodexCredentialActivate,
  useCodexCredentialAdd,
  useCredentialsList,
  useCredentialRemove,
} from '#renderer/domains/settings/runtime/hooks';

export function useSettingsAccounts() {
  const [showTokenInput, setShowTokenInput] = useState(false);
  const [showCodexInput, setShowCodexInput] = useState(false);

  const { data, isLoading } = useCredentialsList();
  const removeMut = useCredentialRemove();
  const codexActiveQuery = useCodexActiveCredential();
  const codexAddMut = useCodexCredentialAdd();
  const codexActivateMut = useCodexCredentialActivate();

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
    codex: {
      items: codexItems,
      isLoading,
      activeAccountId: codexActiveQuery.data?.activeAccountId ?? null,
      matchedCredentialId: codexActiveQuery.data?.matchedCredentialId ?? null,
    },
    mutations: { removeMut, codexAddMut, codexActivateMut },
    /** Back-compat: existing UI reads `.credentials.items`. */
    credentials: { items: all, isLoading },
  };
}
