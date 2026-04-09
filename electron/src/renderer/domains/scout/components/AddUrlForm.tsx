import React, { useState } from 'react';
import { useScoutAdd } from '#renderer/hooks/mutations';
import { Button } from '#renderer/components/ui/button';
import { Input } from '#renderer/components/ui/input';

export function AddUrlForm(): React.ReactElement {
  const [isOpen, setIsOpen] = useState(false);
  const [url, setUrl] = useState('');
  const [title, setTitle] = useState('');
  const addMutation = useScoutAdd();

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!url.trim()) return;
    try {
      await addMutation.mutateAsync({ url: url.trim(), title: title.trim() || undefined });
      setUrl('');
      setTitle('');
      setIsOpen(false);
    } catch {
      // Error surfaced via mutation toast
    }
  };

  if (!isOpen) {
    return (
      <Button data-testid="add-url-btn" size="sm" onClick={() => setIsOpen(true)}>
        + Add URL
      </Button>
    );
  }

  return (
    <form onSubmit={(e) => void handleSubmit(e)} className="flex items-center gap-2">
      <Input
        data-testid="url-input"
        type="text"
        placeholder="https://..."
        value={url}
        onChange={(e) => setUrl(e.target.value)}
        className="h-8 text-sm"
      />
      <Input
        type="text"
        placeholder="Title (optional)"
        value={title}
        onChange={(e) => setTitle(e.target.value)}
        className="h-8 text-sm"
      />
      <Button
        data-testid="submit-url-btn"
        type="submit"
        size="sm"
        disabled={!url.trim()}
        className="bg-success text-background hover:bg-success/90"
      >
        Add
      </Button>
      <Button variant="ghost" size="sm" type="button" onClick={() => setIsOpen(false)}>
        Cancel
      </Button>
    </form>
  );
}
