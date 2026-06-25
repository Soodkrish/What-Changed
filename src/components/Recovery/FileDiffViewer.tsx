import { useState, useEffect, useRef } from "react";
import { X, Loader2, GitBranch } from "lucide-react";
import { diffLines as computeDiffLines } from "diff";
import { getSnapshotContent, getFileContent } from "../../lib/tauri";
import { BlameView } from "./BlameView";

interface FileDiffViewerProps {
  snapshotId: number;
  filePath: string;
  snapshotDate: string;
  onClose: () => void;
}

interface DiffLine {
  type: "added" | "removed" | "unchanged";
  content: string;
}

export function FileDiffViewer({ snapshotId, filePath, snapshotDate, onClose }: FileDiffViewerProps) {
  const [diffLines, setDiffLines] = useState<DiffLine[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [showBlame, setShowBlame] = useState(false);
  const dialogRef = useRef<HTMLDivElement>(null);
  const closeRef = useRef<HTMLButtonElement>(null);

  // Focus trap + Escape key
  useEffect(() => {
    const dialog = dialogRef.current;
    if (!dialog) return;

    // Focus the close button on mount
    closeRef.current?.focus();

    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        e.stopPropagation();
        onClose();
        return;
      }
      // Focus trap: Tab cycles within dialog
      if (e.key === "Tab") {
        const focusable = dialog.querySelectorAll<HTMLElement>(
          'button, [href], input, select, textarea, [tabindex]:not([tabindex="-1"])'
        );
        if (focusable.length === 0) return;
        const first = focusable[0];
        const last = focusable[focusable.length - 1];
        if (e.shiftKey) {
          if (document.activeElement === first) {
            e.preventDefault();
            last.focus();
          }
        } else {
          if (document.activeElement === last) {
            e.preventDefault();
            first.focus();
          }
        }
      }
    };

    document.addEventListener("keydown", handleKeyDown);
    return () => document.removeEventListener("keydown", handleKeyDown);
  }, [onClose]);

  useEffect(() => {
    let cancelled = false;
    const loadDiff = async () => {
      setLoading(true);
      setError(null);
      try {
        const [oldContent, newContent] = await Promise.all([
          getSnapshotContent(snapshotId),
          getFileContent(filePath).catch(() => ""),
        ]);
        if (cancelled) return;

        const changes = computeDiffLines(oldContent || "", newContent || "");
        const result: DiffLine[] = [];

        for (const part of changes) {
          const lines = part.value.split("\n");
          if (lines[lines.length - 1] === "") {
            lines.pop();
          }

          for (const line of lines) {
            if (part.added) {
              result.push({ type: "added", content: line });
            } else if (part.removed) {
              result.push({ type: "removed", content: line });
            } else {
              result.push({ type: "unchanged", content: line });
            }
          }
        }

        if (!cancelled) setDiffLines(result);
      } catch (err) {
        if (!cancelled) setError(err instanceof Error ? err.message : String(err));
      } finally {
        if (!cancelled) setLoading(false);
      }
    };

    loadDiff();
    return () => { cancelled = true; };
  }, [snapshotId, filePath]);

  return (
    <div
      className="fixed inset-0 bg-black/50 flex items-center justify-center z-50 p-4"
      onClick={(e) => { if (e.target === e.currentTarget) onClose(); }}
    >
      <div
        ref={dialogRef}
        role="dialog"
        aria-modal="true"
        aria-label={`Diff viewer for ${filePath}`}
        className="bg-white rounded-xl shadow-2xl w-full max-w-4xl max-h-[80vh] flex flex-col"
      >
        {/* Header */}
        <div className="flex items-center justify-between p-4 border-b border-gray-200">
          <div>
            <h3 className="text-lg font-bold text-gray-900">File Diff</h3>
            <p className="text-sm text-gray-500">
              Snapshot from {new Date(snapshotDate).toLocaleString()}
            </p>
          </div>
          <button
            ref={closeRef}
            onClick={onClose}
            aria-label="Close diff viewer"
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
              <span className="ml-2 text-gray-500">Loading diff...</span>
            </div>
          ) : error ? (
            <div className="p-4 text-red-600 bg-red-50 m-4 rounded-lg">
              {error}
            </div>
          ) : diffLines.length === 0 ? (
            <div className="p-4 text-gray-500 text-center">
              No changes detected
            </div>
          ) : (
            <div className="font-mono text-sm">
              {diffLines.map((line, i) => (
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
          )}
        </div>

        {/* Footer */}
        <div className="p-3 border-t border-gray-200 bg-gray-50 text-sm text-gray-500 flex items-center justify-between">
          <span>
            {diffLines.filter((l) => l.type === "added").length} additions, {diffLines.filter((l) => l.type === "removed").length} deletions
          </span>
          <button
            onClick={() => setShowBlame(!showBlame)}
            className="flex items-center gap-1 px-2.5 py-1 text-xs font-medium text-purple-600 bg-purple-50 rounded-lg hover:bg-purple-100 transition-colors"
          >
            <GitBranch className="w-3.5 h-3.5" />
            {showBlame ? "Hide Blame" : "View Blame"}
          </button>
        </div>
      </div>

      {/* Blame view modal */}
      {showBlame && (
        <BlameView filePath={filePath} onClose={() => setShowBlame(false)} />
      )}
    </div>
  );
}
