import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { open } from '@tauri-apps/plugin-dialog';
import { useState } from 'react';

import { listAccounts } from '@/api/accounts';
import { listParsers, type UploadResult, uploadFile } from '@/api/upload';
import { Button } from '@/components/ui/Button';
import {
  Dialog,
  DialogClose,
  DialogContent,
  DialogTrigger,
} from '@/components/ui/Dialog';
import { Select } from '@/components/ui/Select';
import { Spinner } from '@/components/ui/Spinner';

export function UploadDialog(): React.JSX.Element {
  const queryClient = useQueryClient();
  const [filePath, setFilePath] = useState('');
  const [parserKey, setParserKey] = useState('');
  const [result, setResult] = useState<UploadResult | null>(null);
  const [errorMsg, setErrorMsg] = useState('');
  const [isOpen, setIsOpen] = useState(false);
  const [showLogs, setShowLogs] = useState(false);

  const { data: parsers, isLoading: loadingParsers } = useQuery({
    queryKey: ['parsers'],
    queryFn: listParsers,
  });

  const { data: accounts } = useQuery({
    queryKey: ['accounts'],
    queryFn: listAccounts,
  });

  const displayName = result
    ? (accounts?.find((a) => a.id === result.account_id)?.name ??
      result.account_id)
    : null;

  const uploadMutation = useMutation({
    mutationFn: () => uploadFile(filePath, parserKey),
    onSuccess: (res) => {
      queryClient.invalidateQueries({ queryKey: ['transactions'] });
      queryClient.invalidateQueries({ queryKey: ['accounts'] });
      queryClient.invalidateQueries({ queryKey: ['aggregations'] });
      setResult(res);
      setErrorMsg('');
      setFilePath('');
      setParserKey('');
    },
    onError: (err: unknown) => {
      setErrorMsg(err instanceof Error ? err.message : String(err));
      setResult(null);
    },
  });

  const handleSelectFile = async () => {
    try {
      const selectedParser = parsers?.find((p) => p.key === parserKey);
      const extensions = selectedParser?.format
        ? [selectedParser.format.toLowerCase()]
        : ['xlsx', 'pdf'];

      const selected = await open({
        multiple: false,
        filters: [
          {
            name: selectedParser
              ? `${selectedParser.bank} — ${selectedParser.name}`
              : 'Financial Statements',
            extensions,
          },
        ],
      });
      if (typeof selected === 'string') {
        setFilePath(selected);
      }
    } catch (e: unknown) {
      setErrorMsg(e instanceof Error ? e.message : String(e));
    }
  };

  const parserOpts =
    parsers?.map((p) => ({
      value: p.key,
      label: `${p.bank} — ${p.name} (${p.format.toUpperCase()})`,
    })) ?? [];

  let logCounter = 0;
  const renderedLogs = result?.logs?.map((log) => {
    logCounter += 1;
    return (
      <div key={logCounter} className="whitespace-pre-wrap leading-relaxed">
        {log}
      </div>
    );
  });

  return (
    <Dialog
      open={isOpen}
      onOpenChange={(open) => {
        setIsOpen(open);
        if (!open) {
          setResult(null);
          setErrorMsg('');
          setFilePath('');
          setParserKey('');
          setShowLogs(false);
        }
      }}
    >
      <DialogTrigger
        render={<Button variant="primary">Upload Statement</Button>}
      />
      <DialogContent
        title="Upload Financial Statement"
        description="Import transactions or account summaries from a local file."
      >
        <div className="space-y-4">
          <div className="space-y-1.5">
            <span className="block text-[10px] font-mono uppercase tracking-widest text-muted-foreground font-semibold">
              Select Parser Format
            </span>
            {loadingParsers ? (
              <div className="h-10 flex items-center justify-center border border-border-subtle bg-canvas-raised">
                <Spinner size="sm" />
              </div>
            ) : (
              <Select
                options={parserOpts}
                value={parserKey || undefined}
                onValueChange={(val) => setParserKey(val ?? '')}
                placeholder="Choose parser format…"
              />
            )}
          </div>

          <div className="space-y-1.5">
            <span className="block text-[10px] font-mono uppercase tracking-widest text-muted-foreground font-semibold">
              Statement File
            </span>
            <div className="flex gap-2">
              <input
                type="text"
                readOnly
                placeholder="No file chosen"
                value={filePath}
                className="flex-1 min-w-0 px-3 py-2 text-xs font-mono border border-border-subtle bg-canvas text-foreground focus:outline-none"
              />
              <Button variant="secondary" onClick={handleSelectFile}>
                Browse…
              </Button>
            </div>
          </div>

          {result && (
            <div className="space-y-2">
              <div className="p-3 bg-canvas border border-border-muted text-xs font-mono text-muted-foreground space-y-1">
                <p className="text-growth font-bold uppercase tracking-wider">
                  Upload Successful
                </p>
                <p>Account: {displayName}</p>
                <p>Inserted: {result.inserted}</p>
                <p>Skipped: {result.skipped}</p>
              </div>

              {renderedLogs && renderedLogs.length > 0 && (
                <div className="space-y-1.5">
                  <button
                    type="button"
                    onClick={() => setShowLogs(!showLogs)}
                    className="text-[10px] font-mono uppercase tracking-widest text-muted-foreground hover:text-foreground font-semibold flex items-center gap-1 cursor-pointer bg-transparent border-none p-0"
                  >
                    {showLogs ? 'Hide Import Logs ▲' : 'Show Import Logs ▼'}
                  </button>
                  {showLogs && (
                    <div className="bg-black text-green-400 p-3 h-40 overflow-y-auto font-mono text-[10px] space-y-0.5 border border-border-muted selection:bg-green-700 selection:text-white">
                      {renderedLogs}
                    </div>
                  )}
                </div>
              )}
            </div>
          )}

          {errorMsg && (
            <div className="p-3 bg-canvas border border-expense text-xs font-mono text-expense">
              <p className="font-bold uppercase tracking-wider">
                Upload Failed
              </p>
              <p>{errorMsg}</p>
            </div>
          )}

          <div className="flex justify-end gap-3 pt-2">
            <DialogClose render={<Button variant="ghost">Cancel</Button>} />
            <Button
              variant="primary"
              disabled={!filePath || !parserKey || uploadMutation.isPending}
              onClick={() => uploadMutation.mutate()}
            >
              {uploadMutation.isPending ? (
                <Spinner size="sm" />
              ) : (
                'Process File'
              )}
            </Button>
          </div>
        </div>
      </DialogContent>
    </Dialog>
  );
}
