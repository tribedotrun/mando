import React, { useRef } from 'react';
import { Paperclip } from 'lucide-react';
import { Button } from '#renderer/global/ui/primitives/button';

interface TaskAttachmentButtonProps {
  onImageSelect: (file: File) => void;
  ariaLabel?: string;
  className?: string;
  disabled?: boolean;
  size?: 'icon-xs' | 'icon-sm';
}

export function TaskAttachmentButton({
  onImageSelect,
  ariaLabel = 'Attach image',
  className,
  disabled = false,
  size = 'icon-sm',
}: TaskAttachmentButtonProps): React.ReactElement {
  const fileRef = useRef<HTMLInputElement>(null);

  return (
    <>
      <input
        ref={fileRef}
        type="file"
        accept="image/*"
        className="hidden"
        onChange={(event) => {
          const file = event.target.files?.[0];
          if (file) onImageSelect(file);
          event.target.value = '';
        }}
      />
      <Button
        variant="ghost"
        size={size}
        onClick={() => fileRef.current?.click()}
        disabled={disabled}
        aria-label={ariaLabel}
        className={className}
      >
        <Paperclip size={size === 'icon-sm' ? 16 : 14} />
      </Button>
    </>
  );
}
