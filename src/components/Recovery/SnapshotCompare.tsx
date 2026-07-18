import { useState } from "react";
import { GitCompareArrows, Loader2, X, ArrowRight, Search } from "lucide-react";
import type { FileSnapshotRecord } from "../../lib/tauri";
import {
  getSnapshotsForFile,
  compareAnySnapshots,
  formatBytes,
} from "../../lib/tauri";
import { diffLines as computeDiffLines } from "diff";

interface SnapshotCompareProps {
  onClose: () => void;
}

interface DiffLine {
  type: "added" | "removed" | "unchanged";
  content: string;
  side: "a" | "b" | "both";
}

export function SnapshotCompare({ onClose }: SnapshotCompareProps) {
  const [searchPath, setSearchPath] = useState("");
  const [fileSnapshots, setFileSnapshots] = useState<FileSnapshotRecord[]>([]);
  const [loading, setLoading] = useState(false);
  const [selectedA, setSelectedA] = useState<number | null>(null);
  const [selectedB, setSelectedB] = useState<number | null>(null);
  const [comparing, setComparing] = useState(false);
  const [diffResult, setDiffResult] = useState<DiffLine[] | null>(null);
  const [metaA, setMetaA] = useState<string>("");
  const [metaB, setMetaB] = useState<string>("");

  const handleSearch = async () => {
    if (!searchPath.trim()) return;
    setLoading(true);
    setDiffResult(null);
    setSelectedA(null);
    setSelectedB(null);
    try {
      const snapshots = await getSnapshotsForFile(searchPath);
      setFileSnapshots(snapshots);
    } catch (err) {
      console.error("Failed to load snapshots:", err);
    } finally {
      setLoading(false);
    }
  };

  const handleCompare = async () => {
    if (selectedA === null || selectedB === null || selectedA === selectedB) return;
    setComparing(true);
    try {
      const result = await compareAnySnapshots(selectedA, selectedB);
      if (!result) {
        setDiffResult([]);
        return;
      }
      const [contentA, metaStrA, contentB, metaStrB] = result;
      setMetaA(metaStrA);
      setMetaB(metaStrB);

      // Compute line diff
      const changes = computeDiffLines(contentA, contentB);
      const lines: DiffLine[] = [];
      for (const part of changes) {
        const splitLines = part.value.split("\n");
        if (splitLines[splitLines.length - 1] === "") splitLines.pop();
        for (const line of splitLines) {
          if (part.added) {
            lines.push({ type: "added", content: line, side: "b" });
          } else if (part.removed) {
            lines.push({ type: "removed", content: line, side: "a" });
          } else {
            lines.push({ type: "unchanged", content: line, side: "both" });
          }
        }
      }
      setDiffResult(lines);
    } catch (err) {
      console.error("Failed to compare:", err);
    } finally {
      setComparing(false);
    }
  };

  const snapA = fileSnapshots.find((s) => s.id === selectedA);
  const snapB = fileSnapshots.find((s) => s.id === selectedB);
  const additions = diffResult?.filter((l) => l.type === "added").length || 0;
  const deletions = diffResult?.filter((l) => l.type === "removed").length || 0;

  return (
    <div className="fixed inset-0 bg-black/50 flex items-center justify-center z-50 p-4">
      <div className="bg-white rounded-xl shadow-2xl w-full max-w-5xl max-h-[85vh] flex flex-col">
        {/* Header */}
        <div className="flex items-center justify-between p-4 border-b border-gray-200">
          <div className="flex items-center gap-3">
            <div className="w-8 h-8 rounded-full bg-indigo-100 flex items-center justify-center">
              <GitCompareArrows className="w-4 h-4 text-indigo-600" />
            </div>
            <div>
              <h3 className="text-lg font-bold text-gray-900 dark:text-white">Compare Snapshots</h3>
              <p className="text-sm text-gray-500">Pick any two snapshots to see a full diff</p>
            </div>
          </div>
          <button
            onClick={onClose}
            className="p-2 text-gray-400 hover:text-gray-600 hover:bg-gray-100 rounded-lg transition-colors"
          >
            <X className="w-5 h-5" />
          </button>
        </div>

        {/* Search + Picker */}
        <div className="p-4 border-b border-gray-100 space-y-3">
          <div className="flex gap-2">
            <input
              type="text"
              value={searchPath}
              onChange={(e) => setSearchPath(e.target.value)}
              placeholder="Enter file path to compare versions..."
              className="flex-1 px-3 py-2 text-sm border border-gray-200 rounded-lg focus:outline-none focus:ring-2 focus:ring-brand-500"
              onKeyDown={(e) => e.key === "Enter" && handleSearch()}
            />
            <button
              onClick={handleSearch}
              disabled={loading || !searchPath.trim()}
              className="px-4 py-2 text-sm font-medium text-white bg-brand-500 rounded-lg hover:bg-brand-600 disabled:opacity-50 transition-colors flex items-center gap-1"
            >
              <Search className="w-3.5 h-3.5" />
              {loading ? "Loading..." : "Search"}
            </button>
          </div>

          {/* Snapshot picker */}
          {fileSnapshots.length >= 2 && (
            <div className="flex items-center gap-3">
              <div className="flex-1">
                <label className="text-xs font-medium text-gray-500 mb-1 block">Snapshot A (older)</label>
                <select
                  value={selectedA ?? ""}
                  onChange={(e) => setSelectedA(Number(e.target.value) || null)}
                  className="w-full px-2 py-1.5 text-sm border border-gray-200 rounded-lg focus:outline-none focus:ring-1 focus:ring-brand-500"
                >
                  <option value="">Select snapshot A...</option>
                  {fileSnapshots.map((snap) => (
                    <option key={snap.id} value={snap.id} disabled={snap.id === selectedB}>
                      #{snap.id} — {formatBytes(snap.original_size)} — {new Date(snap.created_at).toLocaleString()}
                    </option>
                  ))}
                </select>
              </div>
              <ArrowRight className="w-5 h-5 text-gray-400 mt-5 flex-shrink-0" />
              <div className="flex-1">
                <label className="text-xs font-medium text-gray-500 mb-1 block">Snapshot B (newer)</label>
                <select
                  value={selectedB ?? ""}
                  onChange={(e) => setSelectedB(Number(e.target.value) || null)}
                  className="w-full px-2 py-1.5 text-sm border border-gray-200 rounded-lg focus:outline-none focus:ring-1 focus:ring-brand-500"
                >
                  <option value="">Select snapshot B...</option>
                  {fileSnapshots.map((snap) => (
                    <option key={snap.id} value={snap.id} disabled={snap.id === selectedA}>
                      #{snap.id} — {formatBytes(snap.original_size)} — {new Date(snap.created_at).toLocaleString()}
                    </option>
                  ))}
                </select>
              </div>
              <button
                onClick={handleCompare}
                disabled={!selectedA || !selectedB || selectedA === selectedB || comparing}
                className="px-4 py-2 mt-5 text-sm font-medium text-white bg-indigo-600 rounded-lg hover:bg-indigo-700 disabled:opacity-50 transition-colors flex items-center gap-1"
              >
                {comparing ? <Loader2 className="w-3.5 h-3.5 animate-spin" /> : <GitCompareArrows className="w-3.5 h-3.5" />}
                Compare
              </button>
            </div>
          )}

          {fileSnapshots.length === 1 && (
            <p className="text-sm text-gray-400">Need at least 2 snapshots to compare. Search for a file with multiple versions.</p>
          )}
        </div>

        {/* Diff results */}
        <div className="flex-1 overflow-auto">
          {comparing && (
            <div className="flex items-center justify-center h-32">
              <Loader2 className="w-6 h-6 text-brand-500 animate-spin" />
              <span className="ml-2 text-gray-500">Comparing snapshots...</span>
            </div>
          )}

          {diffResult && !comparing && (
            <>
              {/* Meta bar */}
              <div className="flex items-center gap-4 px-4 py-2 bg-gray-50 border-b border-gray-100 text-xs text-gray-500">
                <span className="font-medium text-indigo-600">A:</span>
                <span className="truncate">{metaA.split("|")[0]} ({metaA.split("|")[1]})</span>
                <span className="text-gray-300">→</span>
                <span className="font-medium text-indigo-600">B:</span>
                <span className="truncate">{metaB.split("|")[0]} ({metaB.split("|")[1]})</span>
              </div>

              {/* Lines */}
              <div className="font-mono text-sm">
                {diffResult.map((line, i) => (
                  <div
                    key={i}
                    className={`px-4 py-0.5 ${
                      line.type === "added"
                        ? "bg-green-100 text-green-800"
                        : line.type === "removed"
                        ? "bg-red-100 text-red-800"
                        : "bg-white text-gray-700"
                    }`}
                  >
                    <span className="inline-block w-8 text-right mr-4 text-gray-400 select-none">
                      {line.type === "added" ? "+" : line.type === "removed" ? "-" : " "}
                    </span>
                    {line.content}
                  </div>
                ))}
              </div>
            </>
          )}

          {diffResult && !comparing && diffResult.length === 0 && (
            <div className="text-center py-8 text-gray-500">No differences found between these snapshots.</div>
          )}
        </div>

        {/* Footer */}
        {diffResult && !comparing && (
          <div className="p-3 border-t border-gray-200 bg-gray-50 text-sm text-gray-500 flex items-center justify-between">
            <span>
              {snapA && snapB ? (
                <>Comparing #{snapA.id} vs #{snapB.id}</>
              ) : (
                <>Snapshot comparison</>
              )}
            </span>
            <span>{additions} additions, {deletions} deletions</span>
          </div>
        )}
      </div>
    </div>
  );
}
