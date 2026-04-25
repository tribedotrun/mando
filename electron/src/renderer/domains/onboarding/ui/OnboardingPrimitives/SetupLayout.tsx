import React from 'react';

export function SetupLayout({
  step,
  total,
  title,
  subtitle,
  children,
  ...rest
}: {
  step?: number;
  total?: number;
  title: string;
  subtitle?: string;
  children: React.ReactNode;
} & Omit<React.HTMLAttributes<HTMLDivElement>, 'title'>): React.ReactElement {
  return (
    <div className="relative flex h-full bg-background" {...rest}>
      <div className="absolute inset-x-0 top-0 z-10 h-8" style={{ WebkitAppRegion: 'drag' }} />
      <div className="flex w-[300px] shrink-0 flex-col justify-center bg-card px-10 pb-20">
        {step != null && total != null && (
          <div className="mb-6">
            <div className="mb-2.5 flex gap-1.5">
              {Array.from({ length: total }, (_, i) => (
                <div
                  key={i}
                  className={`h-[3px] w-9 rounded ${i < step ? 'bg-foreground' : 'bg-border'}`}
                />
              ))}
            </div>
            <span className="text-label text-muted-foreground">
              Step {step} of {total}
            </span>
          </div>
        )}
        <h2 className="mb-2 text-heading text-foreground">{title}</h2>
        {subtitle && <p className="text-body leading-relaxed text-muted-foreground">{subtitle}</p>}
      </div>
      <div className="flex flex-1 items-center px-16 pb-20">
        <div className="w-full max-w-[480px]">{children}</div>
      </div>
    </div>
  );
}
