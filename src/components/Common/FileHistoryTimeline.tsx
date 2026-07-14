import { useState, useEffect } from "react";
import { Clock, FilePlus, FileEdit, FileX, ArrowRightLeft, ChevronDown, ChevronUp, X } from "lucide-react";
import type { ChangeRecord } from "../../lib/tauri";
import { getFileHistory, parseDbTimestamp } from "../../lib/tauri";

interface FileHistoryTimelineProps {
  filePath: string;
  onClose: () => void;
}

const changeTypeConfig = {
  NEW: { icon: FilePlus, color: "text-emerald-600", bg: "bg-emerald-100", label: "Created" },
  MODIFIED: { icon: FileEdit, color: "text-blue-600", bg: "bg-blue-100", label: "Modified" },
  DELETED: { icon: FileX, color: "text-red-600", bg: "bg-red-100", label: "Deleted" },
  MOVED: { icon: ArrowRightLeft, color: "text-amber-600", bg: "bg-amber-100", label: "Moved" },
};

export function FileHistoryTimeline({ filePath, onClose }: FileHistoryTimelineProps) {
  const [changes, setChanges] = useState<ChangeRecord[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [expandedId, setExpandedId] = useState<number | null>(null);

  useEffect(() => {
    loadHistory();
  }, [filePath]);

  const loadHistory = async () => {
    setLoading(true);
    setError(null);
    try {
      const data = await getFileHistory(filePath);
      setChanges(data);
    } catch (err) {
      setError(String(err));
    } finally {
      setLoading(false);
    }
  };

  const formatDate = (dateStr: string) => {
    const date = parseDbTimestamp(dateStr);
    return date.toLocaleDateString("en-US", {
      month: "short",
      day: "numeric",
      year: "numeric",
    });
  };

  const formatTime = (dateStr: string) => {
    const date = parseDbTimestamp(dateStr);
    return date.toLocaleTimeString("en-US", {
      hour: "numeric",
      minute: "2-digit",
      hour12: true,
    });
  };

  // Group changes by date
  const groupedChanges = changes.reduce((acc, change) => {
    const date = new Date(change.detected_at).toDateString();
    if (!acc[date]) {
      acc[date] = [];
    }
    acc[date].push(change);
    return acc;
  }, {} as Record<string, ChangeRecord[]>);

  return (
    <div className="fixed inset-0 bg-black/50 flex items-center justify-center z-50 p-4">
      <div className="bg-white rounded-xl shadow-2xl w-full max-w-2xl max-h-[80vh] flex flex-col">
        {/* Header */}
        <div className="flex items-center justify-between p-4 border-b border-gray-200">
          <div className="flex items-center gap-3">
            <div className="w-8 h-8 rounded-full bg-brand-100 flex items-center justify-center">
              <Clock className="w-4 h-4 text-brand-600" />
            </div>
            <div>
              <h3 className="text-lg font-bold text-gray-900">File History</h3>
              <p className="text-xs text-gray-500 truncate max-w-xs" title={filePath}>
                {filePath}
              </p>
            </div>
          </div>
          <button
            onClick={onClose}
            className="p-2 text-gray-400 hover:text-gray-600 hover:bg-gray-100 rounded-lg transition-colors"
          >
            <X className="w-5 h-5" />
          </button>
        </div>

        {/* Content */}
        <div className="flex-1 overflow-auto p-4">
          {loading ? (
            <div className="flex items-center justify-center h-32">
              <div className="w-6 h-6 border-2 border-brand-500 border-t-transparent rounded-full animate-spin" />
              <span className="ml-2 text-gray-500">Loading history...</span>
            </div>
          ) : error ? (
            <div className="p-4 text-red-600 bg-red-50 rounded-lg">
              {error}
            </div>
          ) : changes.length === 0 ? (
            <div className="text-center py-8">
              <Clock className="w-8 h-8 mx-auto text-gray-300 mb-2" />
              <p className="text-sm text-gray-500">No history found for this file</p>
            </div>
          ) : (
            <div className="space-y-6">
              {Object.entries(groupedChanges).map(([date, dayChanges]) => (
                <div key={date}>
                  <div className="flex items-center gap-2 mb-3">
                    <div className="w-2 h-2 rounded-full bg-brand-500" />
                    <h4 className="text-sm font-semibold text-gray-700">
                      {formatDate(date)}
                    </h4>
                    <div className="flex-1 h-px bg-gray-200" />
                    <span className="text-xs text-gray-400">
                      {dayChanges.length} change{dayChanges.length !== 1 ? "s" : ""}
                    </span>
                  </div>
                  <div className="ml-3 space-y-2">
                    {dayChanges.map((change) => {
                      const config = changeTypeConfig[change.change_type as keyof typeof changeTypeConfig] || changeTypeConfig.MODIFIED;
                      const Icon = config.icon;
                      const isExpanded = expandedId === change.id;
                      return (
                        <div key={change.id} className="relative pl-6">
                          {/* Timeline line */}
                          <div className="absolute left-0 top-0 bottom-0 w-px bg-gray-200" />
                          {/* Timeline dot */}
                          <div className={`absolute left-0 top-3 w-3 h-3 rounded-full border-2 border-white ${config.bg} transform -translate-x-[5px]`} />
                          <div
                            className={`p-3 rounded-lg cursor-pointer transition-colors ${isExpanded ? "bg-gray-100" : "hover:bg-gray-50"}`}
                            onClick={() => setExpandedId(isExpanded ? null : change.id)}
                          >
                            <div className="flex items-center gap-3">
                              <span className={`inline-flex items-center gap-1 px-2 py-0.5 rounded text-[10px] font-semibold ${config.bg} ${config.color}`}>
                                <Icon className="w-3 h-3" />
                                {config.label}
                              </span>
                              <span className="text-sm font-medium text-gray-900">
                                {change.filename}
                              </span>
                              <span className="text-xs text-gray-400">
                                {formatTime(change.detected_at)}
                              </span>
                              <div className="flex-1" />
                              {isExpanded ? (
                                <ChevronUp className="w-4 h-4 text-gray-400" />
                              ) : (
                                <ChevronDown className="w-4 h-4 text-gray-400" />
                              )}
                            </div>
                            {isExpanded && (
                              <div className="mt-2 text-xs text-gray-500 space-y-1">
                                <p>Full path: {change.file_path}</p>
                                {change.previous_path && (
                                  <p>Previous path: {change.previous_path}</p>
                                )}
                                {change.new_path && (
                                  <p>New path: {change.new_path}</p>
                                )}
                              </div>
                            )}
                          </div>
                        </div>
                      );
                    })}
                  </div>
                </div>
              ))}
            </div>
          )}
        </div>

        {/* Footer */}
        <div className="p-3 border-t border-gray-200 bg-gray-50 text-sm text-gray-500">
          {changes.length} total change{changes.length !== 1 ? "s" : ""} recorded
        </div>
      </div>
    </div>
  );
}
