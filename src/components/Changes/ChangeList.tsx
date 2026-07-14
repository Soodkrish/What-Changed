import type { ChangeRecord } from "../../lib/tauri";
import { FilePlus, FileEdit, FileX, ArrowRightLeft, Search, FolderOpen, ArrowRight } from "lucide-react";
import { useState } from "react";
import { timeAgo } from "../../lib/tauri";
import { invoke } from "@tauri-apps/api/core";

interface ChangeListProps {
  changes: ChangeRecord[];
}

const changeTypeConfig = {
  NEW: { icon: FilePlus, color: "text-emerald-500", bg: "bg-emerald-50", label: "New" },
  MODIFIED: { icon: FileEdit, color: "text-blue-500", bg: "bg-blue-50", label: "Modified" },
  DELETED: { icon: FileX, color: "text-red-500", bg: "bg-red-50", label: "Deleted" },
  MOVED: { icon: ArrowRightLeft, color: "text-amber-500", bg: "bg-amber-50", label: "Moved" },
};

function getFolderFromPath(filePath: string): string {
  const parts = filePath.replace(/\\/g, "/").split("/");
  parts.pop();
  return parts.join("/");
}

async function openInExplorer(path: string) {
  try {
    const folder = getFolderFromPath(path);
    await invoke("open_in_explorer", { path: folder });
  } catch {
    // Silently fail
  }
}

export function ChangeList({ changes }: ChangeListProps) {
  const [filter, setFilter] = useState<string>("ALL");
  const [search, setSearch] = useState("");

  const filtered = changes.filter((c) => {
    const matchesType = filter === "ALL" || c.change_type === filter;
    const matchesSearch =
      search === "" ||
      c.filename.toLowerCase().includes(search.toLowerCase()) ||
      c.file_path.toLowerCase().includes(search.toLowerCase()) ||
      (c.previous_path?.toLowerCase().includes(search.toLowerCase()) ?? false) ||
      (c.new_path?.toLowerCase().includes(search.toLowerCase()) ?? false);
    return matchesType && matchesSearch;
  });

  return (
    <div>
      <div className="flex items-center gap-3 mb-4">
        <div className="relative flex-1 max-w-md">
          <Search className="absolute left-3 top-1/2 -translate-y-1/2 w-4 h-4 text-gray-400" />
          <input
            type="text"
            placeholder="Search files..."
            value={search}
            onChange={(e) => setSearch(e.target.value)}
            className="w-full pl-9 pr-3 py-2 border border-gray-200 rounded-lg text-sm focus:outline-none focus:ring-2 focus:ring-brand-500 focus:border-transparent"
          />
        </div>
        <div className="flex gap-1.5">
          {["ALL", "NEW", "MODIFIED", "DELETED", "MOVED"].map((type) => (
            <button
              key={type}
              onClick={() => setFilter(type)}
              className={`px-3 py-1.5 rounded-lg text-xs font-medium transition-colors ${
                filter === type
                  ? "bg-brand-100 text-brand-700"
                  : "bg-gray-100 text-gray-600 hover:bg-gray-200"
              }`}
            >
              {type}
            </button>
          ))}
        </div>
      </div>

      <div className="overflow-auto max-h-96">
        <table className="w-full">
          <thead className="sticky top-0 bg-white">
            <tr className="border-b border-gray-100">
              <th className="text-left text-xs font-medium text-gray-500 uppercase tracking-wider py-2 px-3">Type</th>
              <th className="text-left text-xs font-medium text-gray-500 uppercase tracking-wider py-2 px-3">File</th>
              <th className="text-left text-xs font-medium text-gray-500 uppercase tracking-wider py-2 px-3">Path</th>
              <th className="text-left text-xs font-medium text-gray-500 uppercase tracking-wider py-2 px-3">Time</th>
              <th className="w-8"></th>
            </tr>
          </thead>
          <tbody className="divide-y divide-gray-50">
            {filtered.length === 0 ? (
              <tr>
                <td colSpan={5} className="text-center py-8 text-gray-400 text-sm">
                  No changes match your filter.
                </td>
              </tr>
            ) : (
              filtered.map((change) => {
                const config = changeTypeConfig[change.change_type] || changeTypeConfig.MODIFIED;
                const Icon = config.icon;
                const isMoved = change.change_type === "MOVED";
                const isDeleted = change.change_type === "DELETED";

                return (
                  <tr key={change.id} className="hover:bg-gray-50">
                    <td className="py-2.5 px-3">
                      <span className={`inline-flex items-center gap-1 px-2 py-0.5 rounded text-xs font-medium ${config.bg} ${config.color}`}>
                        <Icon className="w-3 h-3" />
                        {config.label}
                      </span>
                    </td>
                    <td className="py-2.5 px-3 text-sm font-medium text-gray-900 max-w-[200px] truncate">
                      {change.filename}
                    </td>
                    <td className="py-2.5 px-3 text-sm text-gray-500 max-w-[300px]">
                      {isMoved && change.previous_path ? (
                        <div className="flex items-center gap-1.5 text-xs">
                          <span className="truncate max-w-[130px] text-gray-400" title={change.previous_path}>
                            ...{change.previous_path.split(/[/\\]/).slice(-2).join("/")}
                          </span>
                          <ArrowRight className="w-3 h-3 text-amber-400 flex-shrink-0" />
                          <span className="truncate max-w-[130px] font-medium" title={change.new_path || change.file_path}>
                            ...{(change.new_path || change.file_path).split(/[/\\]/).slice(-2).join("/")}
                          </span>
                        </div>
                      ) : isDeleted ? (
                        <span className="text-red-400 text-xs" title={change.file_path}>
                          Was: ...{change.file_path.split(/[/\\]/).slice(-2).join("/")}
                        </span>
                      ) : (
                        <span className="truncate block" title={change.file_path}>
                          {change.file_path}
                        </span>
                      )}
                    </td>
                    <td className="py-2.5 px-3 text-xs text-gray-400 whitespace-nowrap">
                      {timeAgo(change.detected_at)}
                    </td>
                    <td className="py-2.5 px-1">
                      {!isDeleted && (
                        <button
                          onClick={() => openInExplorer(change.file_path)}
                          className="p-1 text-gray-300 hover:text-brand-500 transition-colors"
                          title="Open in Explorer"
                        >
                          <FolderOpen className="w-3.5 h-3.5" />
                        </button>
                      )}
                    </td>
                  </tr>
                );
              })
            )}
          </tbody>
        </table>
      </div>
    </div>
  );
}
