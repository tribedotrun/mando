import React, { useRef } from 'react';
import { Paperclip, X } from 'lucide-react';
import { Button } from '#renderer/global/ui/button';

interface QAChatImageInputProps {
  image: File | null;
  preview: string | null;
  pending: boolean;
  onSelectFile: (file: File) => void;
  onRemove: () => void;
}

export function QAChatImageInput({
  image,
  preview,
  pending,
  onSelectFile,
  onRemove,
}: QAChatImageInputProps): React.ReactElement {
  const fileRef = useRef<HTMLInputElement>(null);

  return (
    <>
      <input
        ref={fileRef}
        type="file"
        accept="image/*"
        className="hidden"
        onChange={(e) => {
          const file = e.target.files?.[0];
          if (file) onSelectFile(file);
          e.target.value = '';
        }}
      />
      <Button
        type="button"
        variant="ghost"
        size="icon-xs"
        onClick={() => fileRef.current?.click()}
        disabled={pending}
        aria-label="Attach image"
        className="shrink-0 text-muted-foreground"
      >
        <Paperclip size={14} />
      </Button>
      {preview && image && (
        <button
          type="button"
          onClick={onRemove}
          className="flex items-center gap-1.5 rounded-md bg-secondary/60 px-2 py-0.5 text-caption text-muted-foreground transition-colors hover:bg-secondary"
        >
          <img src={preview} alt="" className="h-4 w-4 rounded-sm object-cover" />
          <span className="max-w-[160px] truncate">{image.name}</span>
          <X size={10} className="shrink-0 opacity-60" />
        </button>
      )}
    </>
  );
}
