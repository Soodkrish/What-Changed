import { useDuplicates } from "../../hooks/useDuplicates";
import { formatBytes } from "../../lib/tauri";
import { DuplicateGroup } from "./DuplicateGroup";
import { Loader2, Copy, Trash2 } from "lucide-react";

export function DuplicatesView() {
  const { groups, loading, error } = useDuplicates();

  const totalWasted = groups.reduce(
    (acc, g) => acc + (g.file_count - 1) * g.file_size,
    0,
  );

  if (loading) {
    return (
      <div className="flex items-center justify-center h-96">
        <Loader2 className="w-8 h-8 text-brand-500 animate-spin" />
      </div>
    );
  }

  if (error) {
    return (
      <div className="p-6">
        <div className="bg-red-50 border border-red-200 rounded-lg p-4 text-red-700">
          Error loading duplicates: {error}
        </div>
      </div>
    );
  }

  return (
    <div className="space-y-6">
      <div className="flex items-center justify-between">
        <div>
          <h2 className="text-2xl font-bold text-gray-900">Duplicates</h2>
          <p className="text-sm text-gray-500 mt-1">
            {groups.length} duplicate groups found
          </p>
        </div>
        <div className="flex items-center gap-3">
          {totalWasted > 0 && (
            <div className="flex items-center gap-2 bg-amber-50 text-amber-700 px-4 py-2 rounded-lg">
              <Trash2 className="w-4 h-4" />
              <span className="text-sm font-medium">
                {formatBytes(totalWasted)} wasted
              </span>
            </div>
          )}
        </div>
      </div>

      {groups.length === 0 ? (
        <div className="bg-white rounded-xl border border-gray-200 p-12 text-center">
          <Copy className="w-12 h-12 text-gray-300 mx-auto mb-4" />
          <p className="text-gray-500">No duplicates found.</p>
          <p className="text-sm text-gray-400 mt-1">
            Run a scan to detect duplicate files.
          </p>
        </div>
      ) : (
        <div className="space-y-4">
          {groups.map((group) => (
            <DuplicateGroup key={group.id} group={group} />
          ))}
        </div>
      )}
    </div>
  );
}
