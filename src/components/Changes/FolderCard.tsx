import { useState, useEffect } from "react";
import {
  ChevronDown,
  ChevronUp,
  FolderOpen,
  FilePlus,
  FileEdit,
  FileX,
  ArrowRightLeft,
  ArrowRight,
  History,
  ChevronRight,
  Cloud,
} from "lucide-react";
import type { ScanBatchWithChanges, ChangeRecord } from "../../lib/tauri";
import { timeAgo, isCloudBacked } from "../../lib/tauri";
import { invoke } from "@tauri-apps/api/core";

interface FolderCardProps {
  folderName: string;
  folderPath: string;
  batches: ScanBatchWithChanges[];
  allChanges: ChangeRecord[];
  defaultExpanded?: boolean;
  filter?: string | null;
}

const changeTypeConfig = {
  NEW: { icon: FilePlus, color: "text-emerald-600", bg: "bg-emerald-50", label: "Added" },
  MODIFIED: { icon: FileEdit, color: "text-blue-600", bg: "bg-blue-50", label: "Modified" },
  DELETED: { icon: FileX, color: "text-red-600", bg: "bg-red-50", label: "Deleted" },
  MOVED: { icon: ArrowRightLeft, color: "text-amber-600", bg: "bg-amber-50", label: "Moved" },
};

function formatTime(dateStr: string): string {
  const date = new Date(dateStr);
  return date.toLocaleTimeString("en-US", {
    hour: "numeric",
    minute: "2-digit",
    hour12: true,
  });
}

function getFolderFromPath(filePath: string): string {
  const parts = filePath.replace(/\\/g, "/").split("/");
  parts.pop();
  return parts.join("/");
}

function ChangeRow({ change }: { change: ChangeRecord }) {
  const config = changeTypeConfig[change.change_type] || changeTypeConfig.MODIFIED;
  const Icon = config.icon;
  const isMoved = change.change_type === "MOVED";
  const isDeleted = change.change_type === "DELETED";

  return (
    <div className="flex items-center gap-3 p-2.5 rounded-lg hover:bg-white dark:hover:bg-gray-800 transition-colors group">
      <span className={`inline-flex items-center gap-1 px-2 py-0.5 rounded text-[10px] font-semibold ${config.bg} ${config.color}`}>
        <Icon className="w-3 h-3" />
        {config.label}
      </span>
      <span className="text-sm font-medium text-gray-900 truncate max-w-[200px]">
        {change.filename}
      </span>
      <div className="flex-1 min-w-0">
        {isMoved && change.previous_path ? (
          <div className="flex items-center gap-1 text-xs text-gray-500">
            <span className="truncate max-w-[150px] text-gray-400" title={change.previous_path}>
              ...{change.previous_path.split(/[/\\]/).slice(-2).join("/")}
            </span>
            <ArrowRight className="w-3 h-3 text-amber-400 flex-shrink-0" />
            <span className="truncate max-w-[150px] text-amber-600 font-medium" title={change.new_path || change.file_path}>
              ...{(change.new_path || change.file_path).split(/[/\\]/).slice(-2).join("/")}
            </span>
          </div>
        ) : isDeleted ? (
          <span className="text-xs text-red-400 truncate block" title={change.file_path}>
            Was: ...{change.file_path.split(/[/\\]/).slice(-2).join("/")}
          </span>
        ) : (
          <span className="text-xs text-gray-400 truncate block" title={change.file_path}>
            ...{change.file_path.split(/[/\\]/).slice(-2).join("/")}
          </span>
        )}
      </div>
      <div className="flex-shrink-0 text-[11px] text-gray-400">
        {timeAgo(change.detected_at)}
      </div>
      {!isDeleted && (
        <button
          onClick={async (e) => {
            e.stopPropagation();
            try {
              await invoke("open_in_explorer", { path: getFolderFromPath(change.file_path) });
            } catch (err) { console.warn("Failed to open in explorer:", err); }
          }}
          className="flex-shrink-0 p-1 text-gray-300 hover:text-brand-500 transition-colors opacity-0 group-hover:opacity-100"
          title="Open in Explorer"
        >
          <FolderOpen className="w-3.5 h-3.5" />
        </button>
      )}
    </div>
  );
}

export function FolderCard({
  folderName,
  folderPath,
  batches,
  allChanges,
  defaultExpanded = false,
  filter,
}: FolderCardProps) {
  const [expanded, setExpanded] = useState(defaultExpanded);
  const [historyExpanded, setHistoryExpanded] = useState(false);
  const [isCloud, setIsCloud] = useState(false);

  useEffect(() => {
    isCloudBacked(folderPath).then((result) => setIsCloud(result !== null)).catch(() => {});
  }, [folderPath]);

  // Auto-expand when a filter is active (so user sees filtered changes)
  useEffect(() => {
    if (filter) {
      setExpanded(true);
    }
  }, [filter]);

  // Aggregate stats
  const stats = {
    newCount: allChanges.filter((c) => c.change_type === "NEW").length,
    modifiedCount: allChanges.filter((c) => c.change_type === "MODIFIED").length,
    deletedCount: allChanges.filter((c) => c.change_type === "DELETED").length,
    movedCount: allChanges.filter((c) => c.change_type === "MOVED").length,
  };

  // Latest scan time
  const latestBatch = batches[0];
  const latestTime = latestBatch?.batch.completed_at || latestBatch?.batch.started_at;

  // Filtered changes
  const filteredChanges = filter
    ? allChanges.filter((c) => c.change_type === filter)
    : allChanges;

  // Total files scanned across all scans
  const totalFiles = batches.reduce((sum, b) => sum + (b.batch.total_files || 0), 0);

  return (
    <div className="border border-gray-200 rounded-xl overflow-hidden bg-white dark:bg-gray-900 dark:border-gray-700 hover:border-gray-300 dark:hover:border-gray-600 transition-colors">
      {/* Folder header — always visible */}
      <button
        onClick={() => setExpanded(!expanded)}
        aria-expanded={expanded}
        className="w-full flex items-start gap-4 p-5 text-left hover:bg-gray-50/50 dark:hover:bg-gray-800/50 transition-colors"
      >
        <div className="relative flex-shrink-0 mt-1">
          <div className="w-10 h-10 rounded-full bg-brand-100 dark:bg-brand-900/30 flex items-center justify-center">
            <FolderOpen className="w-5 h-5 text-brand-600 dark:text-brand-400" />
          </div>
        </div>

        <div className="flex-1 min-w-0">
          <div className="flex items-center gap-2">
            <h4 className="text-base font-bold text-gray-900 dark:text-white truncate">{folderName}</h4>
            {isCloud && (
              <span className="inline-flex items-center gap-1 px-1.5 py-0.5 rounded text-[10px] font-semibold bg-cyan-50 text-cyan-700 border border-cyan-200 flex-shrink-0">
                <Cloud className="w-3 h-3" /> Cloud Synced
              </span>
            )}
            {latestTime && (
              <span className="text-xs text-gray-400 flex items-center gap-1 flex-shrink-0">
                {timeAgo(latestTime)}
              </span>
            )}
          </div>
          <p className="text-sm text-gray-500 mt-1 truncate" title={folderPath}>
            {folderPath}
          </p>
          <p className="text-xs text-gray-400 mt-1">
            {batches.length} scan{batches.length !== 1 ? "s" : ""} &middot; {totalFiles.toLocaleString()} files total
          </p>

          {/* Change badges */}
          <div className="flex items-center gap-2 mt-2 flex-wrap">
            {stats.newCount > 0 && (
              <span className="inline-flex items-center gap-1 px-2 py-0.5 rounded-full text-xs font-medium bg-emerald-50 text-emerald-700 border border-emerald-200">
                <FilePlus className="w-3 h-3" /> {stats.newCount}
              </span>
            )}
            {stats.modifiedCount > 0 && (
              <span className="inline-flex items-center gap-1 px-2 py-0.5 rounded-full text-xs font-medium bg-blue-50 text-blue-700 border border-blue-200">
                <FileEdit className="w-3 h-3" /> {stats.modifiedCount}
              </span>
            )}
            {stats.deletedCount > 0 && (
              <span className="inline-flex items-center gap-1 px-2 py-0.5 rounded-full text-xs font-medium bg-red-50 text-red-700 border border-red-200">
                <FileX className="w-3 h-3" /> {stats.deletedCount}
              </span>
            )}
            {stats.movedCount > 0 && (
              <span className="inline-flex items-center gap-1 px-2 py-0.5 rounded-full text-xs font-medium bg-amber-50 text-amber-700 border border-amber-200">
                <ArrowRightLeft className="w-3 h-3" /> {stats.movedCount}
              </span>
            )}
          </div>
        </div>

        <div className="flex-shrink-0 mt-2 text-gray-400">
          {expanded ? <ChevronUp className="w-5 h-5" /> : <ChevronDown className="w-5 h-5" />}
        </div>
      </button>

      {/* Expanded: change list */}
      {expanded && (
        <div className="border-t border-gray-100 bg-gray-50/30 px-5 pb-4 pt-3 space-y-1">
          {filteredChanges.length === 0 ? (
            <p className="text-sm text-gray-400 py-2">No changes match current filter.</p>
          ) : (
            filteredChanges.map((change) => (
              <ChangeRow key={change.id} change={change} />
            ))
          )}
        </div>
      )}

      {/* Scan history — always available when >1 scan */}
      {batches.length > 1 && (
        <div className="border-t border-gray-100">
          <button
            onClick={() => setHistoryExpanded(!historyExpanded)}
            aria-expanded={historyExpanded}
            className="w-full flex items-center gap-2 px-5 py-3 text-left hover:bg-gray-50 transition-colors"
          >
            <History className="w-4 h-4 text-gray-400" />
            <span className="text-xs font-medium text-gray-600">
              {batches.length} scans today — view history
            </span>
            <div className="flex-1" />
            {historyExpanded ? (
              <ChevronUp className="w-4 h-4 text-gray-400" />
            ) : (
              <ChevronDown className="w-4 h-4 text-gray-400" />
            )}
          </button>

          {historyExpanded && (
            <div className="px-5 pb-4 space-y-2">
              {batches.map((batch) => {
                const batchChanges = filter
                  ? batch.changes.filter((c) => c.change_type === filter)
                  : batch.changes;
                const batchStats = {
                  n: batch.changes.filter((c) => c.change_type === "NEW").length,
                  m: batch.changes.filter((c) => c.change_type === "MODIFIED").length,
                  d: batch.changes.filter((c) => c.change_type === "DELETED").length,
                  mv: batch.changes.filter((c) => c.change_type === "MOVED").length,
                };
                const timeStr = batch.batch.completed_at
                  ? formatTime(batch.batch.completed_at)
                  : formatTime(batch.batch.started_at);

                return (
                  <div key={batch.batch.id} className="border border-gray-100 rounded-lg overflow-hidden">
                    <div className="flex items-center gap-3 px-4 py-3 bg-white">
                      <ChevronRight className="w-4 h-4 text-gray-300 flex-shrink-0" />
                      <div className="flex-1 min-w-0">
                        <div className="flex items-center gap-2 flex-wrap">
                          <span className="text-xs font-semibold text-gray-700">Scan #{batch.batch.id}</span>
                          <span className="text-[11px] text-gray-400">
                            {timeStr}
                          </span>
                          {batch.batch.total_files > 0 && (
                            <span className="text-[11px] text-gray-400">
                              {batch.batch.total_files.toLocaleString()} files
                            </span>
                          )}
                        </div>
                        <div className="flex items-center gap-1.5 mt-1">
                          {batchStats.n > 0 && <span className="text-[11px] font-medium text-emerald-600">+{batchStats.n} new</span>}
                          {batchStats.m > 0 && <span className="text-[11px] font-medium text-blue-600">~{batchStats.m} modified</span>}
                          {batchStats.d > 0 && <span className="text-[11px] font-medium text-red-600">-{batchStats.d} deleted</span>}
                          {batchStats.mv > 0 && <span className="text-[11px] font-medium text-amber-600">→{batchStats.mv} moved</span>}
                        </div>
                      </div>
                    </div>
                    {batchChanges.length > 0 && (
                      <div className="px-4 pb-3 space-y-1 border-t border-gray-50">
                        {batchChanges.map((change) => (
                          <ChangeRow key={change.id} change={change} />
                        ))}
                      </div>
                    )}
                  </div>
                );
              })}
            </div>
          )}
        </div>
      )}
    </div>
  );
}
