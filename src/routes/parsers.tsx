import { Editor } from '@monaco-editor/react';
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { createFileRoute } from '@tanstack/react-router';
import { open } from '@tauri-apps/plugin-dialog';
import { useEffect, useState } from 'react';

import {
  classifyTransactions,
  listParserFiles,
  type ParserFileInfo,
  saveParserFile,
  testParserTransform,
} from '@/api/import';
import { ConsoleLogs } from '@/components/import/ConsoleLogs';
import { ImportControls } from '@/components/import/ImportControls';
import { ResultPreview } from '@/components/import/ResultPreview';
import { cn } from '@/lib/util';

interface ParsedTx {
  id: string;
  booking_date: string;
  value_date: string;
  reference: string;
  text: string;
  amount: number;
  balance: number;
  category?: string;
}

interface TestOutput {
  account_id: string;
  month?: string;
  balance?: number;
  transactions?: ParsedTx[];
}

function ImportDebugger(): React.JSX.Element {
  const queryClient = useQueryClient();

  // Resize State
  const [leftWidth, setLeftWidth] = useState(50);
  const [draggingPanel, setDraggingPanel] = useState<
    'horizontal' | 'logs' | null
  >(null);
  const [logsHeight, setLogsHeight] = useState(() =>
    typeof window !== 'undefined' ? Math.round(window.innerHeight * 0.1) : 100,
  );
  const [viewMode, setViewMode] = useState<'table' | 'json'>('table');

  // Debugger State
  const [selectedFile, setSelectedFile] = useState<ParserFileInfo | null>(null);
  const [editorCode, setEditorCode] = useState('');
  const [targetFile, setTargetFile] = useState('');
  const [targetUnit, setTargetUnit] = useState('');
  const [rawResult, setRawResult] = useState('');
  const [parsedResult, setParsedResult] = useState<TestOutput | null>(null);
  const [logsList, setLogsList] = useState<{ id: string; text: string }[]>([]);
  const [errorMsg, setErrorMsg] = useState('');

  // Dragging event handlers
  const handleMouseDownHorizontal = (e: React.MouseEvent) => {
    e.preventDefault();
    setDraggingPanel('horizontal');
  };

  const handleMouseDownLogs = (e: React.MouseEvent) => {
    e.preventDefault();
    setDraggingPanel('logs');
  };

  useEffect(() => {
    if (!draggingPanel) return;

    const handleMouseMove = (e: MouseEvent) => {
      if (draggingPanel === 'horizontal') {
        const percentage = (e.clientX / window.innerWidth) * 100;
        if (percentage >= 25 && percentage <= 75) {
          setLeftWidth(percentage);
        }
      } else if (draggingPanel === 'logs') {
        setLogsHeight((prev) => {
          const next = prev - e.movementY;
          return next >= 50 && next <= 650 ? next : prev;
        });
      }
    };

    const handleMouseUp = () => {
      setDraggingPanel(null);
    };

    document.addEventListener('mousemove', handleMouseMove);
    document.addEventListener('mouseup', handleMouseUp);

    return () => {
      document.removeEventListener('mousemove', handleMouseMove);
      document.removeEventListener('mouseup', handleMouseUp);
    };
  }, [draggingPanel]);

  // Queries & Mutations
  const {
    data: files,
    isLoading: loadingFiles,
    refetch: refetchFiles,
  } = useQuery({
    queryKey: ['parserFiles'],
    queryFn: listParserFiles,
  });

  const saveMutation = useMutation({
    mutationFn: () => {
      if (!selectedFile) return Promise.reject(new Error('No script selected'));
      return saveParserFile(selectedFile.path, editorCode);
    },
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['parsers'] });
      refetchFiles();
      alert('Script saved successfully!');
    },
    onError: (err: unknown) => {
      alert(`Save failed: ${err instanceof Error ? err.message : String(err)}`);
    },
  });

  const testMutation = useMutation({
    mutationFn: () => {
      if (!targetFile)
        return Promise.reject(new Error('Select a bank statement file first'));
      if (!targetUnit)
        return Promise.reject(new Error('Select a parser unit to test'));
      return testParserTransform(targetFile, editorCode, targetUnit);
    },
    onSuccess: async (res) => {
      setRawResult(res.result);
      setLogsList(
        res.logs.map((log, idx) => ({
          id: `log-${idx}-${log.slice(0, 15)}`,
          text: log,
        })),
      );
      setErrorMsg('');
      try {
        const parsed = JSON.parse(res.result) as TestOutput;
        setParsedResult(parsed);

        if (parsed.transactions && parsed.transactions.length > 0) {
          console.log(
            'Input transactions for classification:',
            parsed.transactions,
          );
          const inputs = parsed.transactions.map((t) => ({
            text: t.text,
            amount: t.amount,
            booking_date: t.booking_date,
          }));
          console.log('Serialized classification inputs:', inputs);
          const categories = await classifyTransactions(inputs);
          console.log('Classifier returned categories:', categories);
          const updatedTransactions = parsed.transactions.map((t, idx) => ({
            ...t,
            category: categories[idx] || 'Other',
          }));
          console.log(
            'Updated transactions with categories:',
            updatedTransactions,
          );
          const updatedResult = {
            ...parsed,
            transactions: updatedTransactions,
          };
          setParsedResult(updatedResult);
          setRawResult(JSON.stringify(updatedResult, null, 2));
        } else {
          console.log('No transactions found in parsed output:', parsed);
        }
      } catch (e: unknown) {
        console.error('Import processing error:', e);
        setErrorMsg(e instanceof Error ? e.message : String(e));
      }
    },
    onError: (err: unknown) => {
      setErrorMsg(err instanceof Error ? err.message : String(err));
      setRawResult('');
      setParsedResult(null);
      setLogsList([]);
    },
  });

  // Load script content
  useEffect(() => {
    if (selectedFile) {
      setEditorCode(selectedFile.content);
      if (selectedFile.units.length === 1) {
        setTargetUnit(selectedFile.units[0].key);
      } else {
        setTargetUnit('');
      }
    } else {
      setEditorCode('');
      setTargetUnit('');
    }
  }, [selectedFile]);

  const handleBrowseFile = async () => {
    try {
      const selected = await open({
        multiple: false,
        filters: [
          {
            name: 'Financial Statements',
            extensions: ['xlsx', 'xls', 'csv', 'pdf'],
          },
        ],
      });
      if (typeof selected === 'string') {
        setTargetFile(selected);
      }
    } catch (e: unknown) {
      setErrorMsg(e instanceof Error ? e.message : String(e));
    }
  };

  return (
    <div
      className={cn(
        'flex flex-col h-[calc(100vh-3.5rem)] w-full overflow-hidden bg-canvas relative rounded-none',
        draggingPanel && 'select-none',
      )}
    >
      {/* TOP BAR: Controls Form */}
      <ImportControls
        files={files}
        loadingFiles={loadingFiles}
        selectedFile={selectedFile}
        onSelectedFileChange={setSelectedFile}
        targetUnit={targetUnit}
        onTargetUnitChange={setTargetUnit}
        targetFile={targetFile}
        onBrowseFile={handleBrowseFile}
        onRunTest={() => testMutation.mutate()}
        onSaveScript={() => saveMutation.mutate()}
        testPending={testMutation.isPending}
        savePending={saveMutation.isPending}
      />

      {/* BOTTOM WORKSPACE: Resizable Split Panels */}
      <div className="flex flex-1 w-full overflow-hidden min-h-0 relative rounded-none">
        {/* LEFT SIDE: Monaco Editor Panel */}
        <div
          style={{ width: `${leftWidth}%` }}
          className="h-full flex flex-col border-r border-[#2d2d2d] bg-[#1e1e1e] shrink-0 rounded-none"
        >
          <div className="h-10 flex items-center justify-between px-4 border-b border-[#2d2d2d] bg-[#1a1a1a] shrink-0 rounded-none">
            <span className="text-[10px] font-mono uppercase tracking-wider text-[#a1a1aa] font-bold">
              Monaco Editor Source
            </span>
            {selectedFile && (
              <span className="text-[10px] font-mono text-[#a1a1aa] truncate max-w-xs">
                {selectedFile.filename}
              </span>
            )}
          </div>
          <div
            className={cn(
              'flex-1 w-full overflow-hidden min-h-0 rounded-none',
              draggingPanel && 'pointer-events-none',
            )}
          >
            <Editor
              height="100%"
              language="javascript"
              theme="vs-dark"
              value={editorCode}
              onChange={(val) => setEditorCode(val ?? '')}
              options={{
                fontSize: 13,
                fontFamily: 'Geist Mono, Courier New, monospace',
                minimap: { enabled: false },
                wordWrap: 'on',
                lineNumbers: 'on',
                automaticLayout: true,
                scrollBeyondLastLine: false,
                tabSize: 2,
              }}
            />
          </div>
        </div>

        {/* DRAGGABLE DIVIDER (VERTICAL split) */}
        {/* biome-ignore lint/a11y/noStaticElementInteractions: divider drag is mouse-driven */}
        <div
          onMouseDown={handleMouseDownHorizontal}
          className={cn(
            'w-[1px] h-full cursor-col-resize transition-colors duration-150 relative z-40 shrink-0 select-none outline-none rounded-none',
            draggingPanel === 'horizontal'
              ? 'bg-black'
              : 'bg-border-muted hover:bg-black',
          )}
        />

        {/* RIGHT SIDE: Output + Logs Panels */}
        <div className="flex-1 h-full flex flex-col overflow-hidden min-w-0 rounded-none">
          <ResultPreview
            viewMode={viewMode}
            onViewModeChange={setViewMode}
            parsedResult={parsedResult}
            rawResult={rawResult}
            errorMsg={errorMsg}
            testPending={testMutation.isPending}
            draggingPanel={draggingPanel}
          />

          {/* HORIZONTAL split DIVIDER (Logs) */}
          {/* biome-ignore lint/a11y/noStaticElementInteractions: divider drag is mouse-driven */}
          <div
            onMouseDown={handleMouseDownLogs}
            className={cn(
              'h-[1px] w-full cursor-ns-resize transition-colors duration-150 relative z-35 shrink-0 select-none outline-none rounded-none',
              draggingPanel === 'logs'
                ? 'bg-black'
                : 'bg-border-muted hover:bg-black',
            )}
          />

          <ConsoleLogs
            logsList={logsList}
            logsHeight={logsHeight}
            draggingPanel={draggingPanel}
          />
        </div>
      </div>
    </div>
  );
}

export const Route = createFileRoute('/parsers')({
  component: ImportDebugger,
});
