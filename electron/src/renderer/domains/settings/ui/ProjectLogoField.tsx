import { Button } from '#renderer/global/ui/primitives/button';
import { Label } from '#renderer/global/ui/primitives/label';
import { projectLogoUrl } from '#renderer/domains/settings/runtime/useApi';

interface ProjectLogoFieldProps {
  logoFile: string | null;
  detectingLogo: boolean;
  detectError: string | null;
  onDetect: () => void;
}

export function ProjectLogoField({
  logoFile,
  detectingLogo,
  detectError,
  onDetect,
}: ProjectLogoFieldProps) {
  return (
    <div>
      <Label className="mb-1.5 text-xs text-muted-foreground">Logo</Label>
      <div className="flex items-center gap-3">
        {logoFile && (
          <img
            src={projectLogoUrl(logoFile)}
            alt=""
            width={24}
            height={24}
            className="shrink-0 rounded object-contain"
            onError={(e) => {
              (e.target as HTMLImageElement).style.display = 'none';
            }}
          />
        )}
        <Button variant="outline" size="xs" disabled={detectingLogo} onClick={onDetect}>
          {detectingLogo ? 'Detecting...' : logoFile ? 'Re-detect' : 'Detect logo'}
        </Button>
        {detectError && <span className="text-xs text-destructive">{detectError}</span>}
        {!logoFile && !detectingLogo && !detectError && (
          <span className="text-xs text-muted-foreground">No logo detected</span>
        )}
      </div>
    </div>
  );
}
