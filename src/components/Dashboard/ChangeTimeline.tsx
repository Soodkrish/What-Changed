import type { ChangeRecord } from "../../lib/tauri";
import { FilePlus, FileEdit, FileX, ArrowRightLeft, ArrowRight } from "lucide-react";
import { timeAgo } from "../../lib/tauri";

interface ChangeTimelineProps {
  changes: ChangeRecord[];
}

const changeTypeConfig = {
  NEW: { icon: FilePlus, color: "text-emerald-500", bg: "bg-emerald-50", label: "New" },
  MODIFIED: { icon: FileEdit, color: "text-blue-500", bg: "bg-blue-50", label: "Modified" },
  DELETED: { icon: FileX, color: "text-red-500", bg: "bg-red-50", label: "Deleted" },
  MOVED: { icon: ArrowRightLeft, color: "text-amber-500", bg: "bg-amber-50", label: "Moved" },
};

export function ChangeTimeline({ changes }: ChangeTimelineProps) {
  if (changes.length === 0) {
    return (
      <div className="text-center py-8 text-gray-500">
        No changes detected yet. Click "Scan Now" to start monitoring.
      </div>
    );
  }

  return (
    <div className="space-y-3">
      {changes.map((change) => {
        const config = changeTypeConfig[change.change_type] || changeTypeConfig.MODIFIED;
        const Icon = config.icon;
        const isMoved = change.change_type === "MOVED";
        const isDeleted = change.change_type === "DELETED";

        return (
          <div key={change.id} className="flex items-start gap-3 p-3 rounded-lg hover:bg-gray-50 transition-colors">
            <div className={`${config.bg} p-1.5 rounded-md mt-0.5`}>
              <Icon className={`w-4 h-4 ${config.color}`} />
            </div>
            <div className="flex-1 min-w-0">
              <p className="text-sm font-medium text-gray-900 truncate">
                {change.filename}
              </p>
              {isMoved && change.previous_path ? (
                <div className="flex items-center gap-1 text-xs text-gray-500 mt-0.5">
                  <span className="truncate max-w-[120px] text-gray-400" title={change.previous_path}>
                    ...{change.previous_path.split(/[/\\]/).slice(-2).join("/")}
                  </span>
                  <ArrowRight className="w-3 h-3 text-amber-400 flex-shrink-0" />
                  <span className="truncate max-w-[120px] text-amber-600 font-medium" title={change.new_path || change.file_path}>
                    ...{(change.new_path || change.file_path).split(/[/\\]/).slice(-2).join("/")}
                  </span>
                </div>
              ) : isDeleted ? (
                <p className="text-xs text-red-400 mt-0.5 truncate" title={change.file_path}>
                  Was at: ...{change.file_path.split(/[/\\]/).slice(-2).join("/")}
                </p>
              ) : (
                <p className="text-xs text-gray-500 truncate mt-0.5">
                  {change.file_path}
                </p>
              )}
            </div>
            <div className="text-right flex-shrink-0">
              <span className={`inline-flex items-center px-2 py-0.5 rounded text-xs font-medium ${config.bg} ${config.color}`}>
                {config.label}
              </span>
              <p className="text-xs text-gray-400 mt-1">{timeAgo(change.detected_at)}</p>
            </div>
          </div>
        );
      })}
    </div>
  );
}
