import { useState, useEffect } from "react";
import { X, Loader2, GitBranch } from "lucide-react";
import { getBlameData, parseDbTimestamp } from "../../lib/tauri";
import type { BlameLine } from "../../lib/tauri";

interface BlameViewProps {
  filePath: string;
  onClose: () => void;
}

export function BlameView({ filePath, onClose }: BlameViewProps) {
  const [lines, setLines] = useState<BlameLine[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    const loadBlame = async () => {
      setLoading(true);
      setError(null);
      try {
        const data = await getBlameData(filePath);
        setLines(data);
      } catch (err) {
        setError("Failed to load blame data. Please try again.");
      } finally {
        setLoading(false);
      }
    };
    loadBlame();
  }, [filePath]);

  // Group lines by scan_batch_id for visual grouping
  const batches = new Map<number | null, { lines: BlameLine[]; detected_at: string | null }>();
  for (const line of lines) {
    const key = line.scan_batch_id;
    if (!batches.has(key)) {
      batches.set(key, { lines: [], detected_at: line.detected_at });
    }
    batches.get(key)!.lines.push(line);
  }

  return (
    <div className="fixed inset-0 bg-black/50 flex items-center justify-center z-50 p-4">
      <div className="bg-white rounded-xl shadow-2xl w-full max-w-4xl max-h-[80vh] flex flex-col">
        {/* Header */}
        <div className="flex items-center justify-between p-4 border-b border-gray-200">
          <div className="flex items-center gap-3">
            <div className="w-8 h-8 rounded-full bg-purple-100 flex items-center justify-center">
              <GitBranch className="w-4 h-4 text-purple-600" />
            </div>
            <div>
              <h3 className="text-lg font-bold text-gray-900">Blame View</h3>
              <p className="text-sm text-gray-500 truncate max-w-md" title={filePath}>
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
        <div className="flex-1 overflow-auto">
          {loading ? (
            <div className="flex items-center justify-center h-32">
              <Loader2 className="w-6 h-6 text-brand-500 animate-spin" />
              <span className="ml-2 text-gray-500">Loading blame data...</span>
            </div>
          ) : error ? (
            <div className="p-4 text-red-600 bg-red-50 m-4 rounded-lg">{error}</div>
          ) : lines.length === 0 ? (
            <div className="p-8 text-center text-gray-500">
              <GitBranch className="w-8 h-8 mx-auto text-gray-300 mb-2" />
              <p>No blame data available for this file.</p>
              <p className="text-sm text-gray-400 mt-1">Enable file snapshots in Settings, then scan.</p>
            </div>
          ) : (
            <div className="font-mono text-sm">
              {/* Legend */}
              <div className="flex items-center gap-4 px-4 py-2 bg-gray-50 border-b border-gray-100 text-xs text-gray-500">
                <span className="font-medium">Blame</span>
                <span className="text-gray-400">Each line shows which scan first introduced it</span>
              </div>

              {/* Blame lines */}
              {lines.map((line, i) => {
                const isEvenBatch = line.scan_batch_id ? line.scan_batch_id % 2 === 0 : true;
                const batchColor = isEvenBatch ? "bg-purple-50/50" : "bg-blue-50/50";
                return (
                  <div
                    key={i}
                    className={`flex items-center border-b border-gray-50 hover:bg-gray-50 ${batchColor}`}
                  >
                    {/* Scan ID badge */}
                    <div className="w-20 flex-shrink-0 px-2 py-1 text-[10px] font-medium text-right border-r border-gray-100">
                      {line.scan_batch_id ? (
                        <span className="text-purple-600" title={line.detected_at || undefined}>
                          Scan #{line.scan_batch_id}
                        </span>
                      ) : (
                        <span className="text-gray-400">initial</span>
                      )}
                    </div>

                    {/* Timestamp */}
                    <div className="w-24 flex-shrink-0 px-2 py-1 text-[10px] text-gray-400 border-r border-gray-100 truncate" title={line.detected_at || undefined}>
                      {line.detected_at
                        ? parseDbTimestamp(line.detected_at).toLocaleDateString("en-US", { month: "short", day: "numeric" })
                        : "—"}
                    </div>

                    {/* Line number */}
                    <div className="w-12 flex-shrink-0 px-2 py-1 text-[10px] text-gray-400 text-right border-r border-gray-100">
                      {line.line_number}
                    </div>

                    {/* Content */}
                    <div className="flex-1 px-3 py-1 text-gray-700 whitespace-pre overflow-hidden text-ellipsis">
                      {line.content}
                    </div>
                  </div>
                );
              })}
            </div>
          )}
        </div>

        {/* Footer */}
        <div className="p-3 border-t border-gray-200 bg-gray-50 text-sm text-gray-500 flex items-center justify-between">
          <span>{lines.length} lines total</span>
          <span>{batches.size} scan version{batches.size !== 1 ? "s" : ""}</span>
        </div>
      </div>
    </div>
  );
}
