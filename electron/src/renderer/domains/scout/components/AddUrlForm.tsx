import React, { useState } from 'react';
import { useScoutStore } from '#renderer/domains/scout/stores/scoutStore';

const inputStyle: React.CSSProperties = {
  borderColor: 'var(--color-border)',
  background: 'var(--color-surface-2)',
  color: 'var(--color-text-1)',
};

export function AddUrlForm(): React.ReactElement {
  const [isOpen, setIsOpen] = useState(false);
  const [url, setUrl] = useState('');
  const [title, setTitle] = useState('');
  const add = useScoutStore((s) => s.add);

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!url.trim()) return;
    try {
      await add(url.trim(), title.trim() || undefined);
      setUrl('');
      setTitle('');
      setIsOpen(false);
    } catch {
      // Error already set in scoutStore
    }
  };

  if (!isOpen) {
    return (
      <button
        data-testid="add-url-btn"
        onClick={() => setIsOpen(true)}
        className="rounded px-4 py-2 text-sm font-medium text-bg bg-accent"
      >
        + Add URL
      </button>
    );
  }

  return (
    <form onSubmit={handleSubmit} className="flex items-center gap-2">
      <input
        data-testid="url-input"
        type="text"
        placeholder="https://..."
        value={url}
        onChange={(e) => setUrl(e.target.value)}
        className="rounded border px-3 py-2 text-sm focus:outline-none"
        style={inputStyle}
      />
      <input
        type="text"
        placeholder="Title (optional)"
        value={title}
        onChange={(e) => setTitle(e.target.value)}
        className="rounded border px-3 py-2 text-sm focus:outline-none"
        style={inputStyle}
      />
      <button
        data-testid="submit-url-btn"
        type="submit"
        disabled={!url.trim()}
        className="rounded px-4 py-2 text-sm font-medium text-bg disabled:opacity-50 bg-success"
      >
        Add
      </button>
      <button
        type="button"
        onClick={() => setIsOpen(false)}
        className="rounded px-3 py-2 text-sm text-text-2"
      >
        Cancel
      </button>
    </form>
  );
}
