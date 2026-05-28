import type { ParserFileInfo } from '@/api/import';
import { Button } from '@/components/ui/Button';
import { Select } from '@/components/ui/Select';
import { Spinner } from '@/components/ui/Spinner';

interface ImportControlsProps {
  files: ParserFileInfo[] | undefined;
  loadingFiles: boolean;
  selectedFile: ParserFileInfo | null;
  onSelectedFileChange: (file: ParserFileInfo | null) => void;
  targetUnit: string;
  onTargetUnitChange: (unit: string) => void;
  targetFile: string;
  onBrowseFile: () => void;
  onRunTest: () => void;
  onSaveScript: () => void;
  testPending: boolean;
  savePending: boolean;
}

export function ImportControls({
  files,
  loadingFiles,
  selectedFile,
  onSelectedFileChange,
  targetUnit,
  onTargetUnitChange,
  targetFile,
  onBrowseFile,
  onRunTest,
  onSaveScript,
  testPending,
  savePending,
}: ImportControlsProps): React.JSX.Element {
  const scriptOptions =
    files?.map((f) => ({
      value: f.path,
      label: `${f.filename} (${f.is_builtin ? 'Built-in' : 'User'})`,
    })) ?? [];

  const unitOptions =
    selectedFile?.units?.map((u) => ({
      value: u.key,
      label: `${u.name} [${u.account_type}]`,
    })) ?? [];

  return (
    <div className="flex items-end gap-4 p-3 border-b border-border-muted bg-canvas-raised shrink-0 flex-wrap rounded-none">
      {/* Script to Edit */}
      <div className="flex-1 min-w-[200px] space-y-1">
        <span className="block text-[10px] font-mono uppercase tracking-widest text-muted-foreground font-bold">
          Script to Edit
        </span>
        {loadingFiles ? (
          <div className="h-10 flex items-center justify-center border border-border-subtle bg-canvas rounded-none">
            <Spinner size="sm" />
          </div>
        ) : (
          <Select
            options={scriptOptions}
            value={selectedFile?.path || undefined}
            onValueChange={(val) => {
              const file = files?.find((f) => f.path === val);
              onSelectedFileChange(file || null);
            }}
            placeholder="Select parser script…"
          />
        )}
      </div>

      {/* Unit to Run */}
      <div className="flex-1 min-w-[200px] space-y-1">
        <span className="block text-[10px] font-mono uppercase tracking-widest text-muted-foreground font-bold">
          Unit to Run
        </span>
        <Select
          options={unitOptions}
          value={targetUnit || undefined}
          onValueChange={(val) => onTargetUnitChange(val ?? '')}
          placeholder={selectedFile ? 'Select unit…' : 'Load script first…'}
          disabled={!selectedFile}
        />
      </div>

      {/* Test Statement File */}
      <div className="flex-[2] min-w-[300px] space-y-1">
        <span className="block text-[10px] font-mono uppercase tracking-widest text-muted-foreground font-bold">
          Test Statement File
        </span>
        <div className="flex gap-2">
          <input
            type="text"
            readOnly
            placeholder="No file selected — click browse"
            value={targetFile}
            className="flex-1 min-w-0 px-3 py-2 text-xs font-mono border border-border-subtle bg-canvas text-foreground focus:outline-none rounded-none"
          />
          <Button variant="secondary" onClick={onBrowseFile}>
            Browse…
          </Button>
        </div>
      </div>

      {/* Actions */}
      <div className="flex gap-2 shrink-0">
        <Button
          variant="primary"
          disabled={!targetFile || !targetUnit || testPending}
          onClick={onRunTest}
          className="px-6 rounded-none"
        >
          {testPending ? <Spinner size="sm" /> : 'Run Test Transform'}
        </Button>
        <Button
          variant="secondary"
          disabled={!selectedFile || savePending}
          onClick={onSaveScript}
          className="rounded-none"
        >
          {savePending ? <Spinner size="sm" /> : 'Save Script'}
        </Button>
      </div>
    </div>
  );
}
