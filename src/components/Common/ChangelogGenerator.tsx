import { useState, useRef, useEffect } from "react";
import { FileText, Download, Copy, ChevronDown, ChevronRight } from "lucide-react";
import type { ChangelogEntry } from "../../lib/tauri";
import { getChangelogEntries, generateChangelogMarkdown } from "../../lib/tauri";

export function ChangelogGenerator() {
  const [entries, setEntries] = useState<ChangelogEntry[]>([]);
  const [loading, setLoading] = useState(false);
  const [expandedId, setExpandedId] = useState<number | null>(null);
  const [copied, setCopied] = useState(false);
  const timerRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  useEffect(() => {
    return () => {
      if (timerRef.current) clearTimeout(timerRef.current);
    };
  }, []);

  const handleLoad = async () => {
    setLoading(true);
    try {
      const data = await getChangelogEntries(30);
      setEntries(data);
    } catch (err) {
      console.error("Failed to load changelog:", err);
    } finally {
      setLoading(false);
    }
  };

  const handleCopyMarkdown = async () => {
    try {
      const md = await generateChangelogMarkdown(30);
      await navigator.clipboard.writeText(md);
      if (timerRef.current) clearTimeout(timerRef.current);
      setCopied(true);
      timerRef.current = setTimeout(() => setCopied(false), 2000);
    } catch (err) {
      console.error("Failed to generate markdown:", err);
    }
  };

  const handleDownload = async () => {
    try {
      const md = await generateChangelogMarkdown(30);
      const blob = new Blob([md], { type: "text/markdown" });
      const url = URL.createObjectURL(blob);
      const a = document.createElement("a");
      a.href = url;
      a.download = "CHANGES.md";
      a.click();
      URL.revokeObjectURL(url);
    } catch (err) {
      console.error("Failed to download:", err);
    }
  };

  const formatChangeType = (type: string) => {
    switch (type) {
      case "NEW":
        return { icon: "🆕", label: "New", color: "text-emerald-600 bg-emerald-50" };
      case "MODIFIED":
        return { icon: "📝", label: "Modified", color: "text-blue-600 bg-blue-50" };
      case "DELETED":
        return { icon: "🗑️", label: "Deleted", color: "text-red-600 bg-red-50" };
      case "MOVED":
        return { icon: "📦", label: "Moved", color: "text-amber-600 bg-amber-50" };
      default:
        return { icon: "📄", label: type, color: "text-gray-600 bg-gray-50" };
    }
  };

  return (
    <div className="bg-white rounded-xl border border-gray-200 p-6">
      <div className="flex items-center justify-between mb-4">
        <div className="flex items-center gap-3">
          <FileText className="w-5 h-5 text-green-500" />
          <h3 className="text-lg font-semibold text-gray-900 dark:text-white">Changelog Generator</h3>
        </div>
        <div className="flex items-center gap-2">
          <button
            onClick={handleCopyMarkdown}
            disabled={entries.length === 0}
            className="flex items-center gap-1 px-3 py-1.5 bg-gray-100 text-gray-700 rounded-lg text-sm font-medium hover:bg-gray-200 transition-colors disabled:opacity-50"
          >
            {copied ? <Copy className="w-3.5 h-3.5 text-green-500" /> : <Copy className="w-3.5 h-3.5" />}
            {copied ? "Copied!" : "Copy MD"}
          </button>
          <button
            onClick={handleDownload}
            disabled={entries.length === 0}
            className="flex items-center gap-1 px-3 py-1.5 bg-brand-600 text-white rounded-lg text-sm font-medium hover:bg-brand-700 transition-colors disabled:opacity-50"
          >
            <Download className="w-3.5 h-3.5" />
            Download CHANGES.md
          </button>
        </div>
      </div>
      <p className="text-sm text-gray-500 mb-4">
        Auto-generate a changelog from your scan history. Copy as Markdown or download as CHANGES.md.
      </p>

      {entries.length === 0 && !loading && (
        <button
          onClick={handleLoad}
          className="w-full py-3 border-2 border-dashed border-gray-200 rounded-lg text-sm text-gray-500 hover:border-brand-300 hover:text-brand-600 transition-colors"
        >
          Click to load scan history
        </button>
      )}

      {loading && (
        <div className="text-center py-6 text-gray-400 text-sm">Loading scan history...</div>
      )}

      {entries.length > 0 && (
        <div className="space-y-2 max-h-[500px] overflow-y-auto">
          {entries.map((entry) => (
            <div key={entry.batch_id} className="border border-gray-100 rounded-lg overflow-hidden">
              <button
                onClick={() => setExpandedId(expandedId === entry.batch_id ? null : entry.batch_id)}
                className="w-full flex items-center gap-3 px-4 py-3 text-left hover:bg-gray-50 transition-colors"
              >
                {expandedId === entry.batch_id ? (
                  <ChevronDown className="w-4 h-4 text-gray-400 flex-shrink-0" />
                ) : (
                  <ChevronRight className="w-4 h-4 text-gray-400 flex-shrink-0" />
                )}
                <div className="flex-1 min-w-0">
                  <div className="flex items-center gap-2">
                    <span className="text-sm font-semibold text-gray-900 dark:text-white">{entry.date}</span>
                    <span className="text-xs text-gray-400">Scan #{entry.batch_id}</span>
                  </div>
                  <div className="flex items-center gap-3 mt-1">
                    <span className="text-xs text-gray-500">{entry.folders_scanned}</span>
                    {entry.new_files > 0 && <span className="text-xs font-medium text-emerald-600">+{entry.new_files} new</span>}
                    {entry.modified_files > 0 && <span className="text-xs font-medium text-blue-600">~{entry.modified_files} modified</span>}
                    {entry.deleted_files > 0 && <span className="text-xs font-medium text-red-600">-{entry.deleted_files} deleted</span>}
                    {entry.moved_files > 0 && <span className="text-xs font-medium text-amber-600">→{entry.moved_files} moved</span>}
                  </div>
                </div>
                <span className="text-xs text-gray-400 flex-shrink-0">{entry.changes.length} change{entry.changes.length !== 1 ? "s" : ""}</span>
              </button>

              {expandedId === entry.batch_id && entry.changes.length > 0 && (
                <div className="px-4 pb-3 space-y-1 border-t border-gray-50">
                  {entry.changes.map((change) => {
                    const ft = formatChangeType(change.change_type);
                    return (
                      <div key={change.id} className="flex items-center gap-2 py-1">
                        <span className="text-xs">{ft.icon}</span>
                        <span className={`text-[10px] font-medium px-1.5 py-0.5 rounded ${ft.color}`}>
                          {ft.label}
                        </span>
                        <span className="text-xs text-gray-700 truncate">{change.filename}</span>
                        <span className="text-[10px] text-gray-400 ml-auto">{change.file_path.split(/[/\\]/).slice(-2).join("/")}</span>
                      </div>
                    );
                  })}
                </div>
              )}
            </div>
          ))}
        </div>
      )}
    </div>
  );
}
