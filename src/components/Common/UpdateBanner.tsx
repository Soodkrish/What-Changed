import { useState } from "react";
import { X, Download, ChevronDown, ChevronUp, ExternalLink } from "lucide-react";
import type { UpdateInfo } from "../../lib/tauri";

interface UpdateBannerProps {
  updateInfo: UpdateInfo;
  onDismiss: () => void;
}

export function UpdateBanner({ updateInfo, onDismiss }: UpdateBannerProps) {
  const [expanded, setExpanded] = useState(false);

  const handleDownload = () => {
    if (updateInfo.download_url) {
      window.open(updateInfo.download_url, "_blank");
    }
  };

  return (
    <div className="mx-6 mt-4 mb-2 bg-amber-50 dark:bg-amber-900/20 border border-amber-200 dark:border-amber-700 rounded-xl p-4 shadow-sm">
      <div className="flex items-start justify-between gap-3">
        <div className="flex items-start gap-3 flex-1 min-w-0">
          <div className="flex-shrink-0 w-8 h-8 bg-amber-100 dark:bg-amber-800/40 rounded-lg flex items-center justify-center mt-0.5">
            <span className="text-lg">🔔</span>
          </div>
          <div className="flex-1 min-w-0">
            <div className="flex items-center gap-2 flex-wrap">
              <h3 className="text-sm font-semibold text-amber-800 dark:text-amber-200">
                Update Available
              </h3>
              <span className="inline-flex items-center px-2 py-0.5 rounded-full text-xs font-medium bg-amber-100 dark:bg-amber-800/40 text-amber-700 dark:text-amber-300">
                v{updateInfo.latest_version}
              </span>
              <span className="text-xs text-amber-600 dark:text-amber-400">
                (current: v{updateInfo.current_version})
              </span>
            </div>

            {expanded && updateInfo.release_notes && (
              <div className="mt-3 p-3 bg-white dark:bg-gray-800 rounded-lg border border-amber-100 dark:border-amber-800/40">
                <p className="text-xs font-medium text-gray-500 dark:text-gray-400 mb-2 uppercase tracking-wide">
                  What's New
                </p>
                <div className="text-sm text-gray-700 dark:text-gray-300 whitespace-pre-wrap leading-relaxed max-h-48 overflow-y-auto">
                  {updateInfo.release_notes}
                </div>
              </div>
            )}
          </div>
        </div>

        <button
          onClick={onDismiss}
          className="flex-shrink-0 p-1 text-amber-400 hover:text-amber-600 dark:hover:text-amber-200 rounded-lg hover:bg-amber-100 dark:hover:bg-amber-800/40 transition-colors"
          title="Dismiss"
        >
          <X className="w-4 h-4" />
        </button>
      </div>

      <div className="flex items-center gap-2 mt-3 ml-11">
        <button
          onClick={() => setExpanded(!expanded)}
          className="inline-flex items-center gap-1.5 px-3 py-1.5 text-xs font-medium text-amber-700 dark:text-amber-300 bg-amber-100 dark:bg-amber-800/40 hover:bg-amber-200 dark:hover:bg-amber-700/40 rounded-lg transition-colors"
        >
          {expanded ? (
            <>
              <ChevronUp className="w-3 h-3" />
              Hide Changes
            </>
          ) : (
            <>
              <ChevronDown className="w-3 h-3" />
              View Changes
            </>
          )}
        </button>

        <button
          onClick={handleDownload}
          className="inline-flex items-center gap-1.5 px-3 py-1.5 text-xs font-medium text-white bg-amber-600 hover:bg-amber-700 rounded-lg transition-colors shadow-sm"
        >
          <Download className="w-3 h-3" />
          Download Update
          <ExternalLink className="w-3 h-3 opacity-60" />
        </button>
      </div>
    </div>
  );
}
