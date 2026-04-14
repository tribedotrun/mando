import React, { useState } from 'react';
import { useScoutAdd } from '#renderer/hooks/mutations';
import { Button } from '#renderer/components/ui/button';
import { Input } from '#renderer/components/ui/input';
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogFooter,
} from '#renderer/components/ui/dialog';

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
        <DialogContent showCloseButton={false}>
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
        </DialogContent>
      </Dialog>
    </>
  );
}
