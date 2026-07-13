import { useMutation, useQueryClient } from '@tanstack/react-query';
import { open } from '@tauri-apps/plugin-dialog';
import { useState } from 'react';
import { createSpace, importSpaceData } from '@/api/spaces';
import { Button } from '@/components/ui/Button';
import { Input } from '@/components/ui/Input';
import { Select } from '@/components/ui/Select';
import { Toggle } from '@/components/ui/Toggle';
import type { ImportResult, Space } from '@/types';

interface ImportDialogProps {
  spaces: Space[];
  onClose: () => void;
  initialFilePath?: string;
  initialResult?: ImportResult | null;
  initialMode?: 'new' | 'existing';
}

export const ImportDialog = ({
  spaces,
  onClose,
  initialFilePath = '',
  initialResult = null,
  initialMode = 'new',
}: ImportDialogProps): React.JSX.Element => {
  const queryClient = useQueryClient();
  const [filePath, setFilePath] = useState(initialFilePath);
  const [mode, setMode] = useState<'new' | 'existing'>(initialMode);
  const [newSpaceName, setNewSpaceName] = useState('');
  const [existingSpaceId, setExistingSpaceId] = useState<string | null>(null);
  const [result, setResult] = useState<ImportResult | null>(initialResult);
  const [errorMsg, setErrorMsg] = useState('');

  const pickFile = async () => {
    const selected = await open({
      filters: [{ name: 'JSON', extensions: ['json'] }],
    });
    if (selected) {
      setFilePath(selected);
      setErrorMsg('');
      setResult(null);
    }
  };

  const importMutation = useMutation({
    mutationFn: async () => {
      if (!filePath) throw new Error('No file selected');
      let targetSpaceId: string;
      if (mode === 'new') {
        if (!newSpaceName.trim()) throw new Error('Space name is required');
        const space = await createSpace(newSpaceName.trim());
        targetSpaceId = space.id;
      } else {
        if (!existingSpaceId) throw new Error('No space selected');
        targetSpaceId = existingSpaceId;
      }
      return importSpaceData(targetSpaceId, filePath);
    },
    onSuccess: (res) => {
      setResult(res);
      setErrorMsg('');
      queryClient.invalidateQueries({ queryKey: ['my-spaces'] });
      queryClient.invalidateQueries({ queryKey: ['transactions'] });
      queryClient.invalidateQueries({ queryKey: ['accounts'] });
      queryClient.invalidateQueries({ queryKey: ['aggregations'] });
    },
    onError: (e) => {
      setErrorMsg(String(e));
      setResult(null);
    },
  });

  const canSubmit =
    filePath !== '' &&
    !importMutation.isPending &&
    (mode === 'new' ? newSpaceName.trim() !== '' : existingSpaceId !== null);

  return (
    <div className="flex flex-col gap-4">
      {result ? (
        <>
          <p className="text-xs text-muted-foreground">
            Import complete. Inserted or updated:
          </p>
          <div className="flex flex-col gap-1 text-xs font-mono">
            <span>{result.categories} categories</span>
            <span>{result.accounts} accounts</span>
            <span>{result.transactions} transactions</span>
            <span>{result.account_summaries} account summaries</span>
          </div>
          <div className="flex justify-end gap-2 pt-2 border-t border-border-subtle">
            <Button
              variant="secondary"
              onClick={() => {
                setResult(null);
                setErrorMsg('');
                setFilePath('');
                setExistingSpaceId(null);
                setNewSpaceName('');
              }}
              className="px-4 text-xs rounded-none h-9"
            >
              Import Another
            </Button>
            <Button
              variant="ghost"
              onClick={onClose}
              className="px-4 text-xs rounded-none h-9"
            >
              Done
            </Button>
          </div>
        </>
      ) : (
        <>
          <div className="flex flex-col gap-2">
            <label
              htmlFor="import-file-input"
              className="text-[10px] font-mono uppercase tracking-widest text-muted-foreground font-bold"
            >
              Export File
            </label>
            <div className="flex gap-2">
              <Input
                value={filePath}
                readOnly
                placeholder="No file selected…"
                className="flex-1"
              />
              <Button
                variant="secondary"
                onClick={pickFile}
                className="px-4 text-xs rounded-none h-10 shrink-0"
              >
                Browse
              </Button>
            </div>
          </div>

          <div className="flex flex-col gap-2">
            <label
              htmlFor="import-target-mode"
              className="text-[10px] font-mono uppercase tracking-widest text-muted-foreground font-bold"
            >
              Target
            </label>
            <Toggle
              options={[
                { value: 'new', label: 'New Space' },
                { value: 'existing', label: 'Existing Space' },
              ]}
              value={mode}
              onValueChange={(v) => setMode(v)}
            />
            {mode === 'new' ? (
              <Input
                value={newSpaceName}
                onChange={(e) => setNewSpaceName(e.target.value)}
                placeholder="New space name…"
                className="flex-1"
              />
            ) : (
              <Select
                options={spaces.map((s) => ({ value: s.id, label: s.name }))}
                value={existingSpaceId ?? undefined}
                onValueChange={setExistingSpaceId}
                placeholder="Select space…"
                className="flex-1"
              />
            )}
          </div>

          {errorMsg && (
            <p className="text-xs font-mono text-expense">{errorMsg}</p>
          )}

          <div className="flex justify-end gap-2 pt-2 border-t border-border-subtle">
            <Button
              variant="secondary"
              onClick={onClose}
              className="px-4 text-xs rounded-none h-9"
            >
              Cancel
            </Button>
            <Button
              onClick={() => importMutation.mutate()}
              disabled={!canSubmit}
              className="px-4 text-xs rounded-none h-9"
            >
              {importMutation.isPending ? 'Importing…' : 'Import'}
            </Button>
          </div>
        </>
      )}
    </div>
  );
};
