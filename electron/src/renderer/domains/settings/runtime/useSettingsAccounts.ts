import { useState } from 'react';
import {
  sortCredentials,
  type CredentialSort,
} from '#renderer/domains/settings/service/credentialSort';
import {
  useCodexCredentialAdd,
  useCredentialsList,
  useCredentialRemove,
  useCredentialSetDisabled,
} from '#renderer/domains/settings/runtime/hooks';

export function useSettingsAccounts() {
  const [sort, setSort] = useState<CredentialSort>('reset');
  const [showTokenInput, setShowTokenInput] = useState(false);
  const [showCodexInput, setShowCodexInput] = useState(false);

  const { data, isLoading } = useCredentialsList();
  const removeMut = useCredentialRemove();
  const setDisabledMut = useCredentialSetDisabled();
  const codexAddMut = useCodexCredentialAdd();

  const all = sortCredentials(data?.credentials ?? [], sort);
  const claudeItems = all.filter((c) => c.provider === 'claude');
  const codexItems = all.filter((c) => c.provider === 'codex');

  return {
    sort,
    setSort,
    visibility: {
      showTokenInput,
      setShowTokenInput,
      showCodexInput,
      setShowCodexInput,
    },
    claude: { items: claudeItems, isLoading },
    codex: { items: codexItems, isLoading },
    mutations: { removeMut, setDisabledMut, codexAddMut },
    /** Back-compat: existing UI reads `.credentials.items`. */
    credentials: { items: all, isLoading },
  };
}
