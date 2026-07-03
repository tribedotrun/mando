import React, { useRef } from 'react';
import { FileUp } from 'lucide-react';
import { Input } from '#renderer/global/ui/primitives/input';
import { Label } from '#renderer/global/ui/primitives/label';
import { Button } from '#renderer/global/ui/primitives/button';
import { useAddCodexCredentialForm } from '#renderer/domains/settings/runtime/useAddCodexCredentialForm';

interface AddCodexCredentialFormProps {
  onClose: () => void;
}

export function AddCodexCredentialForm({
  onClose,
}: AddCodexCredentialFormProps): React.ReactElement {
  const form = useAddCodexCredentialForm(onClose);
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
    <div className="space-y-3">
      <p className="text-xs text-muted-foreground/70">
        Run <code className="rounded bg-muted px-1 py-0.5">codex login</code> on a different ChatGPT
        account (use a separate browser profile or a CODEX_HOME-overridden directory), then either
        paste the contents of <code className="rounded bg-muted px-1 py-0.5">auth.json</code> below
        or pick the file directly.
      </p>
      <div>
        <Label className="mb-1.5 text-xs text-muted-foreground">Label</Label>
        <Input
          data-testid="codex-label-input"
          type="text"
          value={form.fields.label}
          onChange={(e) => form.fields.setLabel(e.target.value)}
          placeholder="e.g. work-account"
          autoFocus
        />
      </div>
      <div>
        <div className="mb-1.5 flex items-center justify-between">
          <Label className="text-xs text-muted-foreground">auth.json contents</Label>
          <Button
            type="button"
            variant="outline"
            size="xs"
            onClick={handlePickFile}
            disabled={form.state.pending}
            data-testid="codex-authjson-pick-file"
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
          data-testid="codex-authjson-input"
          value={form.fields.authJson}
          onChange={(e) => form.fields.setAuthJson(e.target.value)}
          placeholder='{"auth_mode":"chatgpt","tokens":{...}}'
          rows={6}
          className="flex min-h-20 w-full rounded-md border border-input bg-transparent px-3 py-2 font-mono text-xs shadow-xs transition-colors focus-visible:ring-1 focus-visible:ring-ring focus-visible:outline-none disabled:cursor-not-allowed disabled:opacity-50"
        />
      </div>
      <div className="flex gap-2">
        <Button
          size="sm"
          disabled={!form.fields.authJson.trim() || !form.fields.label.trim() || form.state.pending}
          onClick={() => void form.actions.handleAdd()}
        >
          {form.state.pending ? 'Validating...' : 'Add Codex Account'}
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
