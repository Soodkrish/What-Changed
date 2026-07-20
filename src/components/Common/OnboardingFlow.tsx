import { useState, useEffect, useRef } from "react";
import { FolderOpen, RefreshCw, BarChart3, Shield, Settings, CheckCircle2, ArrowRight, Check, AlertCircle } from "lucide-react";
import { getMonitoredFolders, openFolderPicker, addMonitoredFolder, scanAllAsync } from "../../lib/tauri";
import { listen } from "@tauri-apps/api/event";
import type { ScanProgress as ScanProgressType } from "../../lib/tauri";

interface OnboardingFlowProps {
  onComplete: () => void;
}

interface Step {
  id: string;
  title: string;
  description: string;
  icon: typeof FolderOpen;
}

const steps: Step[] = [
  { id: "welcome", title: "Welcome to What Changed?", description: "Your personal file system monitor. Let's get you set up in under a minute.", icon: FolderOpen },
  { id: "folder", title: "Add a Folder", description: "Choose a folder to monitor. We'll track every new, modified, deleted, and moved file.", icon: FolderOpen },
  { id: "scan", title: "First Scan", description: "We'll take an initial snapshot. This is the baseline we'll compare against.", icon: RefreshCw },
  { id: "features", title: "What You Get", description: "File tracking, duplicate detection, file recovery, changelogs, and more.", icon: BarChart3 },
  { id: "ready", title: "You're All Set!", description: "Run a scan anytime from the sidebar. Check Settings for more options.", icon: CheckCircle2 },
];

export function OnboardingFlow({ onComplete }: OnboardingFlowProps) {
  const [currentStep, setCurrentStep] = useState(0);
  const [hasFolders, setHasFolders] = useState(false);
  const [scanning, setScanning] = useState(false);
  const [folderPicking, setFolderPicking] = useState(false);
  const [folderAdded, setFolderAdded] = useState(false);
  const [folderError, setFolderError] = useState<string | null>(null);
  const [scanProgress, setScanProgress] = useState<ScanProgressType | null>(null);
  const scanCompleteRef = useRef(false);

  useEffect(() => {
    getMonitoredFolders().then((folders) => {
      if (folders.length > 0) {
        setHasFolders(true);
        setFolderAdded(true);
        // Skip folder step if already has folders
        setCurrentStep(2);
      }
    }).catch(() => {});
  }, []);

  const handleAddFolder = async () => {
    setFolderPicking(true);
    setFolderError(null);
    try {
      const path = await openFolderPicker();
      if (path) {
        await addMonitoredFolder(path);
        setHasFolders(true);
        setFolderAdded(true);
        // Brief pause so user sees the success state
        await new Promise((r) => setTimeout(r, 800));
        setCurrentStep(2);
      }
    } catch (err: any) {
      const msg = typeof err === "string" ? err : err?.message || "Failed to add folder";
      setFolderError(msg);
      console.error("Failed to add folder:", err);
    } finally {
      setFolderPicking(false);
    }
  };

  const handleFirstScan = async () => {
    setScanning(true);
    scanCompleteRef.current = false;
    try {
      await scanAllAsync();
    } catch (err) {
      console.error("Scan failed:", err);
      setScanning(false);
    }
  };

  // Listen for scan progress and completion during onboarding
  useEffect(() => {
    if (!scanning) return;

    const unlistenProgress = listen<ScanProgressType>("scan-progress", (event) => {
      setScanProgress(event.payload);
    });

    const unlistenComplete = listen<void>("scan-complete", () => {
      scanCompleteRef.current = true;
      setScanning(false);
      setScanProgress(null);
      setCurrentStep(3);
    });

    return () => {
      unlistenProgress.then((fn) => fn());
      unlistenComplete.then((fn) => fn());
    };
  }, [scanning]);

  const step = steps[currentStep];
  const Icon = step.icon;

  return (
    <div className="fixed inset-0 bg-white dark:bg-gray-900 z-50 flex items-center justify-center">
      <div className="max-w-lg w-full mx-4">
        {/* Progress dots */}
        <div className="flex justify-center gap-2 mb-8">
          {steps.map((s, i) => (
            <div
              key={s.id}
              className={`w-2 h-2 rounded-full transition-colors ${
                i <= currentStep ? "bg-brand-500" : "bg-gray-200 dark:bg-gray-700"
              }`}
            />
          ))}
        </div>

        {/* Step content */}
        <div className="text-center mb-8">
          <div className="w-16 h-16 rounded-full bg-brand-100 flex items-center justify-center mx-auto mb-4">
            <Icon className="w-8 h-8 text-brand-600" />
          </div>
          <h2 className="text-2xl font-bold text-gray-900 dark:text-white mb-2">{step.title}</h2>
          <p className="text-gray-500 dark:text-gray-400">{step.description}</p>
        </div>

        {/* Error banner */}
        {folderError && (
          <div className="mx-auto max-w-sm mb-4 px-4 py-3 bg-red-50 dark:bg-red-900/20 border border-red-200 dark:border-red-700 rounded-lg flex items-start gap-2">
            <AlertCircle className="w-4 h-4 text-red-500 mt-0.5 flex-shrink-0" />
            <div className="flex-1 min-w-0">
              <p className="text-sm text-red-700 dark:text-red-300">{folderError}</p>
              <button
                onClick={() => setFolderError(null)}
                className="text-xs text-red-500 hover:text-red-700 mt-1 underline"
              >
                Dismiss
              </button>
            </div>
          </div>
        )}

        {/* Step actions */}
        <div className="flex flex-col items-center gap-3">
          {currentStep === 0 && (
            <button
              onClick={() => setCurrentStep(1)}
              className="flex items-center gap-2 px-6 py-3 bg-brand-600 text-white rounded-lg font-medium hover:bg-brand-700 transition-colors"
            >
              Get Started
              <ArrowRight className="w-4 h-4" />
            </button>
          )}

          {currentStep === 1 && (
            <div className="flex flex-col items-center gap-3">
              <div className="flex gap-3">
                <button
                  onClick={handleAddFolder}
                  disabled={folderPicking}
                  className="flex items-center gap-2 px-6 py-3 bg-brand-600 text-white rounded-lg font-medium hover:bg-brand-700 transition-colors disabled:opacity-60"
                >
                  {folderPicking ? (
                    <>
                      <RefreshCw className="w-4 h-4 animate-spin" />
                      Opening folder picker...
                    </>
                  ) : folderAdded ? (
                    <>
                      <Check className="w-4 h-4" />
                      Folder Added ✓
                    </>
                  ) : (
                    <>
                      <FolderOpen className="w-4 h-4" />
                      Choose Folder
                    </>
                  )}
                </button>
                <button
                  onClick={() => setCurrentStep(2)}
                  disabled={folderPicking}
                  className="px-4 py-3 text-sm text-gray-500 hover:text-gray-700 dark:text-gray-400 dark:hover:text-gray-200 disabled:opacity-50"
                >
                  {folderAdded ? "Next" : "Skip for now"}
                </button>
              </div>
              {folderAdded && !folderPicking && (
                <p className="text-xs text-green-600 dark:text-green-400 flex items-center gap-1">
                  <Check className="w-3 h-3" />
                  Folder added successfully — you can add more or continue
                </p>
              )}
            </div>
          )}

          {currentStep === 2 && (
            <div className="flex flex-col items-center gap-4">
              {scanning ? (
                <div className="w-full max-w-sm">
                  <div className="flex items-center gap-3 mb-3">
                    <RefreshCw className="w-5 h-5 text-brand-500 animate-spin" />
                    <div className="flex-1">
                      <p className="text-sm font-medium text-gray-900 dark:text-white">
                        {scanProgress?.phase === "cleanup" ? "Cleaning up deleted files..." :
                         scanProgress?.phase === "snapshot" ? "Taking storage snapshot..." :
                         scanProgress?.phase === "scanning" ? "Scanning files..." : "Scanning..."}
                      </p>
                      {scanProgress?.directory && (
                        <p className="text-xs text-gray-500 truncate">
                          {scanProgress.directory.split(/[/\\]/).pop()}
                        </p>
                      )}
                    </div>
                    <span className="text-sm font-semibold text-brand-600">
                      {scanProgress?.progress_percent ?? 0}%
                    </span>
                  </div>
                  <div className="h-1.5 bg-gray-100 dark:bg-gray-700 rounded-full overflow-hidden">
                    <div
                      className="h-full bg-brand-500 rounded-full transition-all duration-300"
                      style={{ width: `${scanProgress?.progress_percent ?? 0}%` }}
                    />
                  </div>
                  <p className="text-xs text-gray-400 mt-2 text-center">
                    {scanProgress?.total ?? 0} folders — this may take a moment
                  </p>
                </div>
              ) : (
                <div className="flex gap-3">
                  <button
                    onClick={handleFirstScan}
                    disabled={!hasFolders}
                    className="flex items-center gap-2 px-6 py-3 bg-brand-600 text-white rounded-lg font-medium hover:bg-brand-700 transition-colors disabled:opacity-50"
                  >
                    <RefreshCw className="w-4 h-4" />
                    {hasFolders ? "Run First Scan" : "Add a folder first"}
                  </button>
                  <button
                    onClick={() => setCurrentStep(3)}
                    className="px-4 py-3 text-sm text-gray-500 hover:text-gray-700 dark:text-gray-400 dark:hover:text-gray-200"
                  >
                    Skip
                  </button>
                </div>
              )}
            </div>
          )}

          {currentStep === 3 && (
            <div className="space-y-3">
              <div className="grid grid-cols-2 gap-3 text-left mb-4">
                {[
                  { icon: BarChart3, text: "Dashboard with analytics" },
                  { icon: RefreshCw, text: "Duplicate detection" },
                  { icon: Shield, text: "File recovery & snapshots" },
                  { icon: Settings, text: "Custom notification rules" },
                ].map(({ icon: ItemIcon, text }) => (
                  <div key={text} className="flex items-center gap-2 text-sm text-gray-600 dark:text-gray-300">
                    <ItemIcon className="w-4 h-4 text-brand-500" />
                    {text}
                  </div>
                ))}
              </div>
              <button
                onClick={() => setCurrentStep(4)}
                className="flex items-center gap-2 px-6 py-3 bg-brand-600 text-white rounded-lg font-medium hover:bg-brand-700 transition-colors"
              >
                Continue
                <ArrowRight className="w-4 h-4" />
              </button>
            </div>
          )}

          {currentStep === 4 && (
            <button
              onClick={onComplete}
              className="flex items-center gap-2 px-6 py-3 bg-brand-600 text-white rounded-lg font-medium hover:bg-brand-700 transition-colors"
            >
              <CheckCircle2 className="w-4 h-4" />
              Start Using What Changed?
            </button>
          )}
        </div>

        {/* Skip all */}
        {currentStep < 4 && (
          <button
            onClick={onComplete}
            className="block mx-auto mt-6 text-xs text-gray-400 hover:text-gray-600"
          >
            Skip onboarding
          </button>
        )}
      </div>
    </div>
  );
}
