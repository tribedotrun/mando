import React, { useRef } from 'react';
import { FileUp } from 'lucide-react';
import { Button } from '#renderer/global/ui/primitives/button';
import { useUpdateCodexCredentialAuthForm } from '#renderer/domains/settings/runtime/useUpdateCodexCredentialAuthForm';

interface UpdateCodexAuthPasteFormProps {
  credentialId: number;
  onClose: () => void;
}

export function UpdateCodexAuthPasteForm({
  credentialId,
  onClose,
}: UpdateCodexAuthPasteFormProps): React.ReactElement {
  const form = useUpdateCodexCredentialAuthForm(credentialId, onClose);
  const fileRef = useRef<HTMLInputElement>(null);

  const handlePickFile = () => fileRef.current?.click();

  const handleFileChange = async (event: React.ChangeEvent<HTMLInputElement>) => {
    const file = event.target.files?.[0];
    event.target.value = '';
    if (!file) return;
    const text = await file.text();
    form.fields.setAuthJson(text);
  };

  return (
    <div className="space-y-2">
      <div className="flex items-center justify-between">
        <span className="text-xs text-muted-foreground">auth.json contents</span>
        <Button
          type="button"
          variant="outline"
          size="xs"
          onClick={handlePickFile}
          disabled={form.state.pending}
        >
          <FileUp size={12} className="mr-1" />
          Pick auth.json
        </Button>
      </div>
      <input
        ref={fileRef}
        type="file"
        accept="application/json,.json"
        className="hidden"
        onChange={(e) => void handleFileChange(e)}
      />
      <textarea
        data-testid="update-codex-authjson-input"
        value={form.fields.authJson}
        onChange={(e) => form.fields.setAuthJson(e.target.value)}
        placeholder='{"auth_mode":"chatgpt","tokens":{...}}'
        rows={5}
        className="flex min-h-20 w-full rounded-md border border-input bg-transparent px-3 py-2 font-mono text-xs shadow-xs transition-colors focus-visible:ring-1 focus-visible:ring-ring focus-visible:outline-none disabled:cursor-not-allowed disabled:opacity-50"
      />
      <div className="flex gap-2">
        <Button
          size="sm"
          disabled={!form.fields.authJson.trim() || form.state.pending}
          onClick={() => void form.actions.handleUpdate()}
        >
          {form.state.pending ? 'Validating...' : 'Update Auth'}
        </Button>
        <Button
          variant="ghost"
          size="sm"
          onClick={form.actions.handleClose}
          disabled={form.state.pending}
        >
          Cancel
        </Button>
      </div>
    </div>
  );
}
