import { getVersion } from '@tauri-apps/api/app';
import { check, type Update } from '@tauri-apps/plugin-updater';
import { useEffect, useState } from 'react';
import { restartApp } from '@/api/updater';
import { Button } from '@/components/ui/Button';
import { Card, CardBody, CardHeader } from '@/components/ui/Card';
import { Spinner } from '@/components/ui/Spinner';
import { toast } from '@/components/ui/Toast';

type UpdateStatus =
  | 'idle'
  | 'checking'
  | 'available'
  | 'downloading'
  | 'installed';

export function UpdateSettingsCard(): React.JSX.Element {
  const [currentVersion, setCurrentVersion] = useState<string>('');
  const [status, setStatus] = useState<UpdateStatus>('idle');
  const [updateInfo, setUpdateInfo] = useState<Update | null>(null);
  const [downloadedBytes, setDownloadedBytes] = useState<number>(0);
  const [totalBytes, setTotalBytes] = useState<number>(0);

  useEffect(() => {
    getVersion()
      .then(setCurrentVersion)
      .catch((err) => {
        toast.error(
          'Failed to get app version',
          err instanceof Error ? err.message : String(err),
        );
        setCurrentVersion('0.1.0');
      });
  }, []);

  const handleCheckForUpdates = async () => {
    setStatus('checking');
    setUpdateInfo(null);
    setDownloadedBytes(0);
    setTotalBytes(0);

    try {
      const update = await check();
      if (update) {
        setUpdateInfo(update);
        setStatus('available');
      } else {
        toast.success(
          'Up to date',
          `v${currentVersion} is the latest version.`,
        );
        setStatus('idle');
      }
    } catch (err) {
      toast.error(
        'Update check failed',
        err instanceof Error ? err.message : String(err),
      );
      setStatus('idle');
    }
  };

  const handleDownloadAndInstall = async () => {
    if (!updateInfo) return;

    setStatus('downloading');
    setDownloadedBytes(0);
    setTotalBytes(0);

    let progressDownloaded = 0;
    let progressTotal = 0;

    try {
      await updateInfo.downloadAndInstall((event) => {
        if (event.event === 'Started') {
          progressTotal = event.data.contentLength ?? 0;
          setTotalBytes(progressTotal);
        } else if (event.event === 'Progress') {
          progressDownloaded += event.data.chunkLength;
          setDownloadedBytes(progressDownloaded);
        }
      });
      setStatus('installed');
    } catch (err) {
      toast.error(
        'Download failed',
        err instanceof Error ? err.message : String(err),
      );
      setStatus('available');
    }
  };

  const handleRelaunch = async () => {
    try {
      await restartApp();
    } catch (err) {
      toast.error(
        'Relaunch failed',
        err instanceof Error ? err.message : String(err),
      );
      setStatus('installed');
    }
  };

  // Format bytes helper
  const formatMegaBytes = (bytes: number) => {
    return (bytes / (1024 * 1024)).toFixed(1);
  };

  const progressPercent =
    totalBytes > 0
      ? Math.min(100, Math.round((downloadedBytes / totalBytes) * 100))
      : 0;

  return (
    <Card>
      <CardHeader label="Application Updates" />
      <CardBody className="space-y-4 text-left">
        <div className="flex items-center justify-between">
          <div className="space-y-1">
            <p className="text-xs text-muted-foreground font-mono uppercase tracking-widest">
              Current Version
            </p>
            <p className="text-sm font-semibold text-foreground">
              v{currentVersion || '...'}
            </p>
          </div>

          {status === 'idle' && (
            <Button
              variant="secondary"
              onClick={handleCheckForUpdates}
              className="px-4 rounded-none h-9 text-xs font-mono uppercase tracking-widest"
            >
              Check for Updates
            </Button>
          )}

          {status === 'checking' && (
            <div className="flex items-center gap-2 text-xs font-mono text-muted-foreground">
              <Spinner size="sm" />
              <span>Checking...</span>
            </div>
          )}
        </div>

        {status === 'available' && updateInfo && (
          <div className="p-4 bg-canvas border border-border-subtle space-y-3">
            <div className="flex items-start justify-between">
              <div className="space-y-0.5">
                <span className="text-[10px] font-mono uppercase tracking-widest text-primary font-bold">
                  Update Available
                </span>
                <h4 className="text-sm font-bold text-foreground">
                  Version v{updateInfo.version}
                </h4>
                {updateInfo.date && (
                  <p className="text-[10px] text-muted-foreground font-mono">
                    Released: {updateInfo.date}
                  </p>
                )}
              </div>
              <Button
                variant="primary"
                onClick={handleDownloadAndInstall}
                className="px-4 rounded-none h-9 text-xs font-mono uppercase tracking-widest"
              >
                Download & Install
              </Button>
            </div>

            {updateInfo.body && (
              <div className="pt-2 border-t border-border-subtle space-y-1">
                <span className="text-[9px] font-mono uppercase tracking-widest text-muted-foreground">
                  Release Notes
                </span>
                <p className="text-xs text-muted-foreground whitespace-pre-wrap leading-relaxed max-h-32 overflow-y-auto pr-1">
                  {updateInfo.body}
                </p>
              </div>
            )}
          </div>
        )}

        {status === 'downloading' && (
          <div className="p-4 bg-canvas border border-border-subtle space-y-3">
            <div className="flex items-center justify-between text-xs">
              <span className="font-mono text-muted-foreground uppercase tracking-wider">
                Downloading update...
              </span>
              <span className="font-mono font-semibold">
                {totalBytes > 0
                  ? `${formatMegaBytes(downloadedBytes)} MB / ${formatMegaBytes(totalBytes)} MB`
                  : 'Starting...'}
              </span>
            </div>

            {/* Custom Premium Progress Bar */}
            <div className="w-full bg-border-subtle h-1.5 overflow-hidden">
              <div
                className="bg-primary h-full transition-all duration-300 ease-out"
                style={{ width: `${progressPercent}%` }}
              />
            </div>
          </div>
        )}

        {status === 'installed' && (
          <div className="p-4 bg-primary/5 border border-primary/20 space-y-3">
            <div className="space-y-1">
              <span className="text-[10px] font-mono uppercase tracking-widest text-primary font-bold">
                Update Ready
              </span>
              <p className="text-xs text-muted-foreground leading-relaxed">
                The update has been downloaded and installed successfully.
                Relaunch the application to apply.
              </p>
            </div>
            <div className="flex justify-end">
              <Button
                variant="primary"
                onClick={handleRelaunch}
                className="px-5 rounded-none h-9 text-xs font-mono uppercase tracking-widest cursor-pointer"
              >
                Relaunch Application
              </Button>
            </div>
          </div>
        )}
      </CardBody>
    </Card>
  );
}
