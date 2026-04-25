import React from 'react';

interface SpinnerIconProps {
  size?: number;
  className?: string;
}

/** SVG-based spinner with a translucent track circle. */
export function SpinnerIcon({ size = 14, className }: SpinnerIconProps): React.ReactElement {
  return (
    <svg
      className={`animate-spin ${className ?? ''}`}
      width={size}
      height={size}
      viewBox="0 0 14 14"
      fill="none"
    >
      <circle cx="7" cy="7" r="5.5" stroke="currentColor" strokeWidth="2" opacity="0.3" />
      <path
        d="M12.5 7a5.5 5.5 0 0 0-5.5-5.5"
        stroke="currentColor"
        strokeWidth="2"
        strokeLinecap="round"
      />
    </svg>
  );
}
