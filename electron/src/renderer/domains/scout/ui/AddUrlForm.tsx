import React, { useState } from 'react';
import { useScoutAdd } from '#renderer/domains/scout/runtime/hooks';
import { Button } from '#renderer/global/ui/primitives/button';
import { Input } from '#renderer/global/ui/primitives/input';
import {
  Dialog,
  DialogContentPlain,
  DialogHeader,
  DialogTitle,
  DialogFooter,
} from '#renderer/global/ui/primitives/dialog';

export function AddUrlForm(): React.ReactElement {
  const [isOpen, setIsOpen] = useState(false);
  const [url, setUrl] = useState('');
  const addMutation = useScoutAdd();

  const close = () => {
    setIsOpen(false);
    setUrl('');
  };

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    if (addMutation.isPending) return;
    const trimmedUrl = url.trim();
    if (!trimmedUrl) return;
    try {
      await addMutation.mutateAsync({ url: trimmedUrl });
      close();
    } catch {
      // Error surfaced via mutation toast
    }
  };

  return (
    <>
      <Button data-testid="add-url-btn" size="sm" onClick={() => setIsOpen(true)}>
        + Add URL
      </Button>
      <Dialog open={isOpen} onOpenChange={(open) => !open && !addMutation.isPending && close()}>
        <DialogContentPlain>
          <form onSubmit={(e) => void handleSubmit(e)}>
            <DialogHeader>
              <DialogTitle>Add URL</DialogTitle>
            </DialogHeader>
            <div className="py-4">
              <Input
                data-testid="url-input"
                type="text"
                placeholder="https://..."
                value={url}
                onChange={(e) => setUrl(e.target.value)}
                autoFocus
              />
            </div>
            <DialogFooter>
              <Button
                variant="outline"
                type="button"
                onClick={close}
                disabled={addMutation.isPending}
              >
                Cancel
              </Button>
              <Button
                data-testid="submit-url-btn"
                type="submit"
                disabled={!url.trim() || addMutation.isPending}
              >
                {addMutation.isPending ? 'Adding...' : 'Add'}
              </Button>
            </DialogFooter>
          </form>
        </DialogContentPlain>
      </Dialog>
    </>
  );
}
