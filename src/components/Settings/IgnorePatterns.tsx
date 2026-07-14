import { useState, useEffect } from "react";
import { Plus, X, Shield, Info } from "lucide-react";
import {
  addIgnorePattern,
  removeIgnorePattern,
  getIgnorePatterns,
  type IgnorePattern,
  type MonitoredFolder,
} from "../../lib/tauri";

interface IgnorePatternsProps {
  folders: MonitoredFolder[];
}

export function IgnorePatterns({ folders }: IgnorePatternsProps) {
  const [patterns, setPatterns] = useState<IgnorePattern[]>([]);
  const [newPattern, setNewPattern] = useState("");
  const [patternType, setPatternType] = useState<"glob" | "contains">("glob");
  const [selectedFolder, setSelectedFolder] = useState<number | null>(null);
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    loadPatterns();
  }, []);

  const loadPatterns = async () => {
    setLoading(true);
    try {
      const data = await getIgnorePatterns();
      setPatterns(data);
    } catch (err) {
      console.error("Failed to load ignore patterns:", err);
    } finally {
      setLoading(false);
    }
  };

  const handleAdd = async () => {
    if (!newPattern.trim()) return;
    const folderId = selectedFolder ?? (folders.length > 0 ? folders[0].id : null);
    if (!folderId) return;

    try {
      await addIgnorePattern(folderId, newPattern.trim(), patternType);
      setNewPattern("");
      await loadPatterns();
    } catch (err) {
      console.error("Failed to add pattern:", err);
    }
  };

  const handleRemove = async (id: number) => {
    try {
      await removeIgnorePattern(id);
      await loadPatterns();
    } catch (err) {
      console.error("Failed to remove pattern:", err);
    }
  };

  return (
    <div className="bg-white rounded-xl border border-gray-200 p-6 dark:bg-gray-900 dark:border-gray-700">
      <div className="flex items-center gap-3 mb-4">
        <Shield className="w-5 h-5 text-brand-500" />
        <h3 className="text-lg font-semibold text-gray-900 dark:text-white">Ignore Patterns</h3>
      </div>
      <p className="text-sm text-gray-500 dark:text-gray-400 mb-4">
        Skip files matching these patterns during scans. Use glob syntax (*.log, **/*.tmp) or simple contains matching.
      </p>

      {/* Add pattern form */}
      <div className="flex gap-2 mb-4 items-stretch">
        <select
          value={selectedFolder ?? ""}
          onChange={(e) => setSelectedFolder(Number(e.target.value) || null)}
          className="h-9 px-3 text-sm border border-gray-200 rounded-lg dark:bg-gray-800 dark:border-gray-600 dark:text-white shrink-0"
        >
          <option value="">All folders</option>
          {folders.map((f) => (
            <option key={f.id} value={f.id}>
              {f.path.split(/[/\\]/).pop()}
            </option>
          ))}
        </select>
        <select
          value={patternType}
          onChange={(e) => setPatternType(e.target.value as "glob" | "contains")}
          className="h-9 px-3 text-sm border border-gray-200 rounded-lg dark:bg-gray-800 dark:border-gray-600 dark:text-white shrink-0"
        >
          <option value="glob">Glob</option>
          <option value="contains">Contains</option>
        </select>
        <input
          type="text"
          value={newPattern}
          onChange={(e) => setNewPattern(e.target.value)}
          placeholder="*.log, **/*.tmp, temp..."
          className="flex-1 min-w-0 h-9 px-3 text-sm border border-gray-200 rounded-lg dark:bg-gray-800 dark:border-gray-600 dark:text-white dark:placeholder-gray-500"
          onKeyDown={(e) => e.key === "Enter" && handleAdd()}
        />
        <button
          onClick={handleAdd}
          disabled={!newPattern.trim()}
          aria-label="Add ignore pattern"
          title="Add pattern"
          className="flex items-center justify-center h-9 w-9 shrink-0 bg-brand-600 text-white rounded-lg text-sm font-medium hover:bg-brand-700 disabled:opacity-50 transition-colors"
        >
          <Plus className="w-4 h-4" />
        </button>
      </div>

      {/* Pattern list */}
      {loading ? (
        <p className="text-sm text-gray-400 dark:text-gray-500">Loading patterns...</p>
      ) : patterns.length === 0 ? (
        <div className="text-center py-4">
          <Info className="w-6 h-6 mx-auto text-gray-300 dark:text-gray-600 mb-2" />
          <p className="text-sm text-gray-500 dark:text-gray-400">No ignore patterns yet</p>
          <p className="text-xs text-gray-400 dark:text-gray-500 mt-1">
            Add patterns to skip files during scans (e.g., *.log, node_modules, *.tmp)
          </p>
        </div>
      ) : (
        <div className="space-y-1">
          {patterns.map((p) => {
            const folder = folders.find((f) => f.id === p.folder_id);
            return (
              <div
                key={p.id}
                className="flex items-center gap-3 p-2 rounded-lg hover:bg-gray-50 dark:hover:bg-gray-800 group"
              >
                <span className="px-2 py-0.5 rounded text-[10px] font-semibold bg-gray-100 text-gray-600 dark:bg-gray-800 dark:text-gray-400 uppercase">
                  {p.pattern_type}
                </span>
                <span className="text-sm font-mono text-gray-700 dark:text-gray-300">{p.pattern}</span>
                {folder && (
                  <span className="text-xs text-gray-400 dark:text-gray-500">
                    in {folder.path.split(/[/\\]/).pop()}
                  </span>
                )}
                <div className="flex-1" />
                <button
                  onClick={() => handleRemove(p.id)}
                  className="p-1 text-gray-300 hover:text-red-500 transition-colors opacity-0 group-hover:opacity-100 dark:text-gray-600 dark:hover:text-red-400"
                >
                  <X className="w-3.5 h-3.5" />
                </button>
              </div>
            );
          })}
        </div>
      )}
    </div>
  );
}
