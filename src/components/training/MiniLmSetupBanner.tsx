import { listen } from '@tauri-apps/api/event';
import { useEffect, useState } from 'react';
import { ensureMinilm, minilmStatus } from '@/api/training';
import { Button } from '@/components/ui/Button';

interface DownloadProgress {
  file: string;
  step: number;
  total: number;
}

export function MiniLmSetupBanner(): React.JSX.Element | null {
  const [needed, setNeeded] = useState<boolean | null>(null);
  const [downloading, setDownloading] = useState(false);
  const [progress, setProgress] = useState<DownloadProgress | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    minilmStatus()
      .then((ready) => setNeeded(!ready))
      .catch(() => setNeeded(false));
  }, []);

  useEffect(() => {
    if (!downloading) return;
    let unlisten: (() => void) | null = null;
    listen<DownloadProgress>('minilm://progress', (e) => {
      setProgress(e.payload);
    }).then((fn) => {
      unlisten = fn;
    });
    return () => {
      unlisten?.();
    };
  }, [downloading]);

  if (!needed) return null;

  const start = async () => {
    setDownloading(true);
    setError(null);
    try {
      await ensureMinilm();
      setNeeded(false);
    } catch (e) {
      setError(String(e));
    } finally {
      setDownloading(false);
      setProgress(null);
    }
  };

  return (
    <div className="border border-border-subtle bg-canvas-raised px-6 py-5 flex flex-col gap-3">
      <div className="flex items-start justify-between gap-4">
        <div className="flex flex-col gap-1">
          <span className="text-sm font-mono font-semibold text-foreground">
            MiniLM weights required
          </span>
          <span className="text-xs font-mono text-muted-foreground">
            The classifier needs the sentence-transformers/all-MiniLM-L6-v2
            model (~90 MB). Downloaded once and cached locally.
          </span>
        </div>
        <Button
          variant="secondary"
          onClick={start}
          disabled={downloading}
          className="shrink-0 text-xs h-8 px-4"
        >
          {downloading ? 'Downloading…' : 'Download model'}
        </Button>
      </div>

      {downloading && progress && (
        <div className="flex flex-col gap-1">
          <div className="h-1 w-full bg-border-subtle overflow-hidden">
            <div
              className="h-full bg-foreground transition-all duration-300"
              style={{ width: `${(progress.step / progress.total) * 100}%` }}
            />
          </div>
          <span className="text-[10px] font-mono text-muted-foreground">
            {progress.file} ({progress.step}/{progress.total})
          </span>
        </div>
      )}

      {error && (
        <span className="text-[10px] font-mono text-expense">{error}</span>
      )}
    </div>
  );
}
