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
  const [filePaths, setFilePaths] = useState<string[]>([]);
  const [parserKey, setParserKey] = useState('');
  const [results, setResults] = useState<UploadResult[]>([]);
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

  const accountName = (id: string): string =>
    accounts?.find((a) => a.id === id)?.name ?? id;

  const totals = results.reduce(
    (acc, r) => ({
      inserted: acc.inserted + r.inserted,
      skipped: acc.skipped + r.skipped,
    }),
    { inserted: 0, skipped: 0 },
  );

  const reset = () => {
    setFilePaths([]);
    setParserKey('');
    setResults([]);
    setErrorMsg('');
    setShowLogs(false);
  };

  const uploadMutation = useMutation({
    mutationFn: async () => {
      const collected: UploadResult[] = [];
      for (const fp of filePaths) {
        const r = await uploadFile(fp, parserKey);
        collected.push(r);
      }
      return collected;
    },
    onSuccess: (res) => {
      queryClient.invalidateQueries({ queryKey: ['transactions'] });
      queryClient.invalidateQueries({ queryKey: ['accounts'] });
      queryClient.invalidateQueries({ queryKey: ['aggregations'] });
      setResults(res);
      setErrorMsg('');
      setFilePaths([]);
    },
    onError: (err: unknown) => {
      setErrorMsg(err instanceof Error ? err.message : String(err));
    },
  });

  const handleSelectFiles = async () => {
    try {
      const selectedParser = parsers?.find((p) => p.key === parserKey);
      const extensions = selectedParser?.format
        ? [selectedParser.format.toLowerCase()]
        : ['xlsx', 'pdf'];

      const selected = await open({
        multiple: true,
        filters: [
          {
            name: selectedParser
              ? `${selectedParser.bank} — ${selectedParser.name}`
              : 'Financial Statements',
            extensions,
          },
        ],
      });
      if (Array.isArray(selected)) {
        setFilePaths(selected);
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
  const renderedLogs = results.flatMap((r) =>
    (r.logs ?? []).map((log) => {
      logCounter += 1;
      return (
        <div key={logCounter} className="whitespace-pre-wrap leading-relaxed">
          {log}
        </div>
      );
    }),
  );

  const hasResults = results.length > 0;
  const isProcessing = uploadMutation.isPending;

  return (
    <Dialog
      open={isOpen}
      onOpenChange={(open) => {
        setIsOpen(open);
        if (!open) reset();
      }}
    >
      <DialogTrigger
        render={<Button variant="primary">Upload Statement</Button>}
      />
      <DialogContent
        title="Upload Financial Statement"
        description="Import transactions or account summaries from one or more local files."
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
              Statement Files
            </span>
            <div className="flex gap-2">
              <input
                type="text"
                readOnly
                placeholder="No files chosen"
                value={
                  filePaths.length > 0
                    ? `${filePaths.length} file${filePaths.length === 1 ? '' : 's'} selected`
                    : ''
                }
                className="flex-1 min-w-0 px-3 py-2 text-xs font-mono border border-border-subtle bg-canvas text-foreground focus:outline-none"
              />
              <Button variant="secondary" onClick={handleSelectFiles}>
                Browse…
              </Button>
            </div>
            {filePaths.length > 0 && (
              <ul className="text-[10px] font-mono text-muted-foreground space-y-0.5 max-h-24 overflow-y-auto border border-border-subtle p-2">
                {filePaths.map((p) => (
                  <li key={p} className="truncate">
                    {p}
                  </li>
                ))}
              </ul>
            )}
          </div>

          {hasResults && (
            <div className="space-y-2">
              <div className="p-3 bg-canvas border border-border-muted text-xs font-mono text-muted-foreground space-y-1">
                <p className="text-growth font-bold uppercase tracking-wider">
                  Upload Successful — {results.length} file
                  {results.length === 1 ? '' : 's'}
                </p>
                {results.length === 1 && (
                  <p>Account: {accountName(results[0].account_id)}</p>
                )}
                <p>Inserted: {totals.inserted}</p>
                <p>Skipped: {totals.skipped}</p>
                {results.length > 1 && (
                  <ul className="pt-1 space-y-0.5">
                    {results.map((r) => (
                      <li key={r.account_id}>
                        {accountName(r.account_id)} — inserted {r.inserted},
                        skipped {r.skipped}
                      </li>
                    ))}
                  </ul>
                )}
              </div>

              {renderedLogs.length > 0 && (
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
            {hasResults && !isProcessing ? (
              <>
                <Button variant="secondary" onClick={reset}>
                  Import Another
                </Button>
                <DialogClose render={<Button variant="ghost">Close</Button>} />
              </>
            ) : (
              <>
                <DialogClose render={<Button variant="ghost">Cancel</Button>} />
                <Button
                  variant="primary"
                  disabled={
                    filePaths.length === 0 || !parserKey || isProcessing
                  }
                  onClick={() => uploadMutation.mutate()}
                >
                  {isProcessing ? (
                    <>
                      <Spinner size="sm" />
                      <span className="ml-2">
                        Processing {filePaths.length} file
                        {filePaths.length === 1 ? '' : 's'}…
                      </span>
                    </>
                  ) : (
                    `Process ${filePaths.length > 0 ? `${filePaths.length} ` : ''}File${filePaths.length === 1 ? '' : 's'}`
                  )}
                </Button>
              </>
            )}
          </div>
        </div>
      </DialogContent>
    </Dialog>
  );
}
