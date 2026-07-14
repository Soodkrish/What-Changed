import { useState } from "react";
import {
  ChevronDown,
  FolderOpen,
  FilePlus,
  FileEdit,
  FileX,
  ArrowRightLeft,
  Clock,
  ArrowRight,
  ChevronUp,
} from "lucide-react";
import type { ScanBatchWithChanges } from "../../lib/tauri";
import { formatBytes, timeAgo, parseDbTimestamp } from "../../lib/tauri";
import { invoke } from "@tauri-apps/api/core";

interface CommitCardProps {
  batch: ScanBatchWithChanges;
  compact?: boolean;
  defaultExpanded?: boolean;
}

const changeTypeConfig = {
  NEW: { icon: FilePlus, color: "text-emerald-600", bg: "bg-emerald-50", border: "border-emerald-200", label: "Added" },
  MODIFIED: { icon: FileEdit, color: "text-blue-600", bg: "bg-blue-50", border: "border-blue-200", label: "Modified" },
  DELETED: { icon: FileX, color: "text-red-600", bg: "bg-red-50", border: "border-red-200", label: "Deleted" },
  MOVED: { icon: ArrowRightLeft, color: "text-amber-600", bg: "bg-amber-50", border: "border-amber-200", label: "Moved" },
};

function getFolderFromPath(filePath: string): string {
  const parts = filePath.replace(/\\/g, "/").split("/");
  parts.pop();
  return parts.join("/");
}

function formatTime(dateStr: string): string {
  const date = parseDbTimestamp(dateStr);
  return date.toLocaleTimeString("en-US", {
    hour: "numeric",
    minute: "2-digit",
    hour12: true,
  });
}

function getBatchStats(batch: ScanBatchWithChanges) {
  const changes = batch.changes;
  return {
    newCount: changes.filter((c) => c.change_type === "NEW").length,
    modifiedCount: changes.filter((c) => c.change_type === "MODIFIED").length,
    deletedCount: changes.filter((c) => c.change_type === "DELETED").length,
    movedCount: changes.filter((c) => c.change_type === "MOVED").length,
  };
}

function getFolderTitle(batch: ScanBatchWithChanges): string {
  // Use the folders_scanned field from DB (folder names)
  if (batch.batch.folders_scanned && batch.batch.folders_scanned.trim()) {
    return batch.batch.folders_scanned;
  }
  // Fallback: extract from changes
  const folders = new Set<string>();
  for (const change of batch.changes) {
    const parts = change.file_path.replace(/\\/g, "/").split("/");
    parts.pop();
    const last = parts[parts.length - 1];
    if (last) folders.add(last);
  }
  if (folders.size === 0) return "Unknown folder";
  return [...folders].join(", ");
}

function getFoldersChanged(changes: { file_path: string }[]): Map<string, number> {
  const folders = new Map<string, number>();
  for (const change of changes) {
    const folder = getFolderFromPath(change.file_path);
    const shortFolder = folder.split(/[/\\]/).slice(-1)[0] || folder;
    folders.set(shortFolder, (folders.get(shortFolder) || 0) + 1);
  }
  return folders;
}

export function CommitCard({ batch, compact = false, defaultExpanded = false }: CommitCardProps) {
  const [expanded, setExpanded] = useState(defaultExpanded);
  const { batch: scanBatch, changes } = batch;
  const stats = getBatchStats(batch);
  const title = getFolderTitle(batch);
  const folders = getFoldersChanged(changes);

  const timeStr = scanBatch.completed_at
    ? formatTime(scanBatch.completed_at)
    : formatTime(scanBatch.started_at);

  // COMPACT: summary-only card for Recent section (no expand)
  if (compact) {
    return (
      <div className="group">
        <div className="w-full flex items-center gap-3 p-3 rounded-lg bg-gray-50 border border-gray-100">
          <div className="flex-shrink-0 w-8 h-8 rounded-lg bg-brand-50 flex items-center justify-center">
            <FolderOpen className="w-4 h-4 text-brand-500" />
          </div>
          <div className="flex-1 min-w-0">
            <div className="flex items-center gap-2">
              <span className="text-sm font-semibold text-gray-900 truncate" title={title}>
                {title}
              </span>
            </div>
            <div className="flex items-center gap-2 mt-0.5 flex-wrap">
              {stats.newCount > 0 && (
                <span className="inline-flex items-center gap-1 text-xs text-emerald-600">
                  <FilePlus className="w-3 h-3" /> {stats.newCount} new
                </span>
              )}
              {stats.modifiedCount > 0 && (
                <span className="inline-flex items-center gap-1 text-xs text-blue-600">
                  <FileEdit className="w-3 h-3" /> {stats.modifiedCount} modified
                </span>
              )}
              {stats.deletedCount > 0 && (
                <span className="inline-flex items-center gap-1 text-xs text-red-600">
                  <FileX className="w-3 h-3" /> {stats.deletedCount} deleted
                </span>
              )}
              {stats.movedCount > 0 && (
                <span className="inline-flex items-center gap-1 text-xs text-amber-600">
                  <ArrowRightLeft className="w-3 h-3" /> {stats.movedCount} moved
                </span>
              )}
              <span className="text-[11px] text-gray-400 flex items-center gap-1">
                <Clock className="w-3 h-3" /> {timeStr}
              </span>
            </div>
          </div>
        </div>
      </div>
    );
  }

  // FULL: expandable commit card for All Changes section
  return (
    <div className="border border-gray-200 rounded-xl overflow-hidden bg-white hover:border-gray-300 transition-colors">
      {/* Card Header */}
      <button
        onClick={() => setExpanded(!expanded)}
        className="w-full flex items-start gap-4 p-5 text-left hover:bg-gray-50/50 transition-colors"
      >
        {/* Folder icon */}
        <div className="relative flex-shrink-0 mt-1">
          <div className="w-10 h-10 rounded-full bg-brand-100 flex items-center justify-center">
            <FolderOpen className="w-5 h-5 text-brand-600" />
          </div>
          <div className="absolute -bottom-1 -right-1 w-4 h-4 rounded-full bg-white border-2 border-gray-200 flex items-center justify-center">
            <span className="text-[8px] font-bold text-gray-500">
              {changes.length}
            </span>
          </div>
        </div>

        {/* Content */}
        <div className="flex-1 min-w-0">
          <div className="flex items-center gap-2">
            <h4 className="text-base font-bold text-gray-900 truncate" title={title}>
              {title}
            </h4>
            <span className="text-xs text-gray-400 flex items-center gap-1 flex-shrink-0">
              <Clock className="w-3 h-3" />
              {timeStr}
            </span>
          </div>

          {/* Summary line */}
          <p className="text-sm text-gray-500 mt-1">
            {scanBatch.total_files > 0 && (
              <>{scanBatch.total_files.toLocaleString()} files scanned</>
            )}
            {scanBatch.total_size > 0 && (
              <> &middot; {formatBytes(scanBatch.total_size)}</>
            )}
            {scanBatch.total_files === 0 && scanBatch.total_size === 0 && (
              <>Scanned {scanBatch.folder_count} folder{scanBatch.folder_count !== 1 ? "s" : ""}</>
            )}
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

          {/* Folder chips */}
          <div className="flex items-center gap-1.5 mt-2 flex-wrap">
            {[...folders.entries()].map(([folder, count]) => (
              <span
                key={folder}
                className="inline-flex items-center gap-1 px-2 py-0.5 rounded-md text-[11px] font-medium bg-gray-100 text-gray-600"
              >
                <FolderOpen className="w-3 h-3" />
                {folder} ({count})
              </span>
            ))}
          </div>
        </div>

        {/* Expand arrow */}
        <div className="flex-shrink-0 mt-2 text-gray-400">
          {expanded ? (
            <ChevronUp className="w-5 h-5" />
          ) : (
            <ChevronDown className="w-5 h-5" />
          )}
        </div>
      </button>

      {/* Expanded: file list */}
      {expanded && (
        <div className="border-t border-gray-100 bg-gray-50/30">
          <div className="p-4 space-y-1.5 max-h-80 overflow-auto">
            {changes.map((change) => {
              const config = changeTypeConfig[change.change_type] || changeTypeConfig.MODIFIED;
              const Icon = config.icon;
              const isMoved = change.change_type === "MOVED";
              const isDeleted = change.change_type === "DELETED";

              return (
                <div
                  key={change.id}
                  className="flex items-center gap-3 p-2.5 rounded-lg bg-white border border-gray-100 hover:border-gray-200 transition-colors group"
                >
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
                          const folder = getFolderFromPath(change.file_path);
                          await invoke("open_in_explorer", { path: folder });
                        } catch {
                          // silently fail
                        }
                      }}
                      className="flex-shrink-0 p-1 text-gray-300 hover:text-brand-500 transition-colors opacity-0 group-hover:opacity-100"
                      title="Open in Explorer"
                    >
                      <FolderOpen className="w-3.5 h-3.5" />
                    </button>
                  )}
                </div>
              );
            })}
          </div>
        </div>
      )}
    </div>
  );
}
