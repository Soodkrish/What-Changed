import { useEffect, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { ScanProgress as ScanProgressType } from "../../lib/tauri";
import { Loader2, Check } from "lucide-react";

interface ScanProgressProps {
  onComplete?: () => void;
}

export function ScanProgress({ onComplete }: ScanProgressProps) {
  const [progress, setProgress] = useState<ScanProgressType | null>(null);
  const [isComplete, setIsComplete] = useState(false);
  const timerRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  useEffect(() => {
    return () => {
      if (timerRef.current) clearTimeout(timerRef.current);
    };
  }, []);

  useEffect(() => {
    const unlisten = listen<ScanProgressType>("scan-progress", (event) => {
      setProgress(event.payload);

      if (event.payload.phase === "complete") {
        setIsComplete(true);
        if (timerRef.current) clearTimeout(timerRef.current);
        timerRef.current = setTimeout(() => {
          setProgress(null);
          setIsComplete(false);
          timerRef.current = null;
          if (onComplete) onComplete();
        }, 2000);
      }
    });

    return () => {
      unlisten.then((fn) => fn());
      if (timerRef.current) clearTimeout(timerRef.current);
    };
  }, [onComplete]);

  if (!progress) return null;

  const phaseLabels: Record<string, string> = {
    scanning: "Scanning directory",
    cleanup: "Cleaning up deleted files",
    snapshot: "Taking storage snapshot",
    detecting_duplicates: "Detecting duplicates",
    complete: "Scan complete!",
  };

  const getDirectoryShortName = (path: string) => {
    if (!path) return "";
    const parts = path.split(/[/\\]/);
    return parts[parts.length - 1] || path;
  };

  return (
    <div className="fixed bottom-6 left-6 right-6 md:left-auto md:right-6 md:w-96 bg-white rounded-xl shadow-lg border border-gray-200 p-4 z-50">
      <div className="flex items-center gap-3 mb-3">
        {!isComplete ? (
          <Loader2 className="w-5 h-5 text-brand-500 animate-spin" />
        ) : (
          <div className="w-5 h-5 rounded-full bg-emerald-100 flex items-center justify-center">
            <Check className="w-3 h-3 text-emerald-600" />
          </div>
        )}
        <div className="flex-1 min-w-0">
          <p className="text-sm font-medium text-gray-900 dark:text-white">
            {phaseLabels[progress.phase] || progress.phase}
          </p>
          {progress.directory && (
            <p className="text-xs text-gray-500 truncate">
              {getDirectoryShortName(progress.directory)}
            </p>
          )}
        </div>
        <span className="text-sm font-semibold text-brand-600">
          {progress.progress_percent}%
        </span>
      </div>

      {/* Progress bar */}
      <div className="h-1.5 bg-gray-100 rounded-full overflow-hidden">
        <div
          className={`h-full rounded-full transition-all duration-300 ${
            isComplete ? "bg-emerald-500" : "bg-brand-500"
          }`}
          style={{ width: `${progress.progress_percent}%` }}
        />
      </div>

      {/* Stats */}
      {progress.total > 1 && (
        <div className="mt-2 text-xs text-gray-500">
          Folder {progress.current} of {progress.total}
        </div>
      )}
    </div>
  );
}
