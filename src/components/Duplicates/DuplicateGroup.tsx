import type { DuplicateGroupRecord } from "../../lib/tauri";
import { formatBytes } from "../../lib/tauri";
import { File, ChevronDown, ChevronUp } from "lucide-react";
import { useState } from "react";

interface DuplicateGroupProps {
  group: DuplicateGroupRecord;
}

export function DuplicateGroup({ group }: DuplicateGroupProps) {
  const [expanded, setExpanded] = useState(false);

  return (
    <div className="bg-white rounded-xl border border-gray-200 overflow-hidden">
      <button
        onClick={() => setExpanded(!expanded)}
        className="w-full flex items-center justify-between p-4 hover:bg-gray-50 transition-colors"
      >
        <div className="flex items-center gap-4">
          <div className="bg-amber-50 p-2 rounded-lg">
            <File className="w-5 h-5 text-amber-600" />
          </div>
          <div className="text-left">
            <p className="text-sm font-medium text-gray-900">
              {group.file_count} copies of {formatBytes(group.file_size)}
            </p>
            <p className="text-xs text-gray-500 mt-0.5">
              {formatBytes((group.file_count - 1) * group.file_size)} wasted
            </p>
          </div>
        </div>
        {expanded ? (
          <ChevronUp className="w-5 h-5 text-gray-400" />
        ) : (
          <ChevronDown className="w-5 h-5 text-gray-400" />
        )}
      </button>

      {expanded && (
        <div className="border-t border-gray-100 divide-y divide-gray-50">
          {group.file_paths.map((path, i) => (
            <div key={i} className="px-4 py-3 flex items-center gap-3">
              <div className="w-6 h-6 rounded bg-gray-100 flex items-center justify-center text-xs font-medium text-gray-500">
                {i + 1}
              </div>
              <p className="text-sm text-gray-700 truncate flex-1">{path}</p>
            </div>
          ))}
        </div>
      )}
    </div>
  );
}
