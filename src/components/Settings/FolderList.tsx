import type { MonitoredFolder } from "../../lib/tauri";
import { removeMonitoredFolder, toggleMonitoredFolder } from "../../lib/tauri";
import { Folder, Trash2, ToggleLeft, ToggleRight } from "lucide-react";

interface FolderListProps {
  folders: MonitoredFolder[];
  onRefresh: () => void;
  onFolderRemoved?: (folderName: string) => void;
}

export function FolderList({ folders, onRefresh, onFolderRemoved }: FolderListProps) {
  const handleRemove = async (path: string) => {
    const folderName = path.split(/[/\\]/).pop() || path;
    await removeMonitoredFolder(path);
    if (onFolderRemoved) {
      onFolderRemoved(folderName);
    } else {
      onRefresh();
    }
  };

  const handleToggle = async (id: number, enabled: boolean) => {
    await toggleMonitoredFolder(id, !enabled);
    onRefresh();
  };

  if (folders.length === 0) {
    return (
      <div className="text-center py-8 text-gray-500">
        <Folder className="w-10 h-10 text-gray-300 mx-auto mb-3" />
        <p>No folders being monitored.</p>
        <p className="text-sm text-gray-400 mt-1">Click "Add Folder" to get started.</p>
      </div>
    );
  }

  return (
    <div className="space-y-2">
      {folders.map((folder) => (
        <div
          key={folder.id}
          className="flex items-center gap-3 p-3 rounded-lg border border-gray-100 hover:bg-gray-50"
        >
          <Folder className="w-5 h-5 text-brand-500 flex-shrink-0" />
          <div className="flex-1 min-w-0">
            <p className="text-sm font-medium text-gray-900 truncate">{folder.path}</p>
            <p className="text-xs text-gray-400">
              Added {new Date(folder.added_at).toLocaleDateString()}
            </p>
          </div>
          <button
            onClick={() => handleToggle(folder.id, folder.enabled)}
            className="flex-shrink-0"
          >
            {folder.enabled ? (
              <ToggleRight className="w-8 h-8 text-brand-600" />
            ) : (
              <ToggleLeft className="w-8 h-8 text-gray-400" />
            )}
          </button>
          <button
            onClick={() => handleRemove(folder.path)}
            className="p-1.5 text-gray-400 hover:text-red-500 transition-colors"
          >
            <Trash2 className="w-4 h-4" />
          </button>
        </div>
      ))}
    </div>
  );
}
