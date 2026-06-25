import { useState } from "react";
import { RotateCcw, FileX, RefreshCw, CheckCircle } from "lucide-react";
import type { RecycleBinEntry } from "../../lib/tauri";
import { formatBytes, timeAgo, restoreFromRecycleBin } from "../../lib/tauri";

interface RecycleBinPanelProps {
  entries: RecycleBinEntry[];
  onRefresh: () => void;
}

export function RecycleBinPanel({ entries, onRefresh }: RecycleBinPanelProps) {
  const [restoring, setRestoring] = useState<number | null>(null);
  const [restored, setRestored] = useState<Set<number>>(new Set());

  const handleRestore = async (entry: RecycleBinEntry) => {
    const confirmed = window.confirm(
      `Restore "${entry.filename}" to its original location?\n\n` +
      `Original path: ${entry.original_path}\n` +
      `If a file already exists there, it will be backed up with a .pre-restore suffix first.`
    );
    if (!confirmed) return;

    setRestoring(entry.id);
    try {
      await restoreFromRecycleBin(entry.id);
      setRestored((prev) => new Set(prev).add(entry.id));
      onRefresh();
    } catch (err) {
      window.alert(`Restore failed: ${err}`);
    } finally {
      setRestoring(null);
    }
  };

  return (
    <div className="bg-white rounded-xl border border-gray-200 overflow-hidden">
      <div className="flex items-center justify-between p-5 border-b border-gray-100">
        <div className="flex items-center gap-3">
          <div className="w-10 h-10 rounded-full bg-orange-100 flex items-center justify-center">
            <RotateCcw className="w-5 h-5 text-orange-600" />
          </div>
          <div>
            <h3 className="text-base font-bold text-gray-900">Recycle Bin</h3>
            <p className="text-sm text-gray-500">
              {entries.length} recoverable file{entries.length !== 1 ? "s" : ""}
            </p>
          </div>
        </div>
        <button
          onClick={onRefresh}
          className="p-2 text-gray-400 hover:text-brand-500 hover:bg-gray-50 rounded-lg transition-colors"
          title="Refresh recycle bin"
        >
          <RefreshCw className="w-4 h-4" />
        </button>
      </div>

      {entries.length === 0 ? (
        <div className="p-8 text-center text-gray-500">
          <FileX className="w-8 h-8 mx-auto text-gray-300 mb-2" />
          <p className="text-sm">No recoverable files found in recycle bin</p>
        </div>
      ) : (
        <div className="divide-y divide-gray-50">
          {entries.map((entry) => (
            <div
              key={entry.id}
              className="flex items-center gap-3 px-5 py-3 hover:bg-gray-50/50 transition-colors"
            >
              <FileX className="w-4 h-4 text-orange-400 flex-shrink-0" />
              <div className="flex-1 min-w-0">
                <p className="text-sm font-medium text-gray-900 truncate">
                  {entry.filename}
                </p>
                <p className="text-xs text-gray-400 truncate" title={entry.original_path}>
                  Was: ...{entry.original_path.split(/[/\\]/).slice(-2).join("/")}
                </p>
              </div>
              <span className="text-xs text-gray-400 flex-shrink-0">
                {formatBytes(entry.original_size)}
              </span>
              <span className="text-xs text-gray-400 flex-shrink-0">
                {timeAgo(entry.deleted_at)}
              </span>
              {restored.has(entry.id) ? (
                <span className="flex items-center gap-1 text-xs text-emerald-600 flex-shrink-0">
                  <CheckCircle className="w-3.5 h-3.5" /> Restored
                </span>
              ) : (
                <button
                  onClick={() => handleRestore(entry)}
                  disabled={restoring === entry.id}
                  className="flex-shrink-0 px-3 py-1.5 text-xs font-medium text-orange-600 bg-orange-50 border border-orange-200 rounded-lg hover:bg-orange-100 transition-colors disabled:opacity-50"
                >
                  {restoring === entry.id ? "Restoring..." : "Restore"}
                </button>
              )}
            </div>
          ))}
        </div>
      )}
    </div>
  );
}
