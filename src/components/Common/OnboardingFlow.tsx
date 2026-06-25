import { useState, useEffect } from "react";
import { FolderOpen, RefreshCw, BarChart3, Shield, Settings, CheckCircle2, ArrowRight } from "lucide-react";
import { getMonitoredFolders, openFolderPicker, addMonitoredFolder, scanAll } from "../../lib/tauri";

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

  useEffect(() => {
    getMonitoredFolders().then((folders) => {
      if (folders.length > 0) {
        setHasFolders(true);
        // Skip folder step if already has folders
        setCurrentStep(2);
      }
    }).catch(() => {});
  }, []);

  const handleAddFolder = async () => {
    try {
      const path = await openFolderPicker();
      if (path) {
        await addMonitoredFolder(path);
        setHasFolders(true);
        setCurrentStep(2);
      }
    } catch (err) {
      console.error("Failed to add folder:", err);
    }
  };

  const handleFirstScan = async () => {
    setScanning(true);
    try {
      await scanAll();
      setCurrentStep(3);
    } catch (err) {
      console.error("Scan failed:", err);
    } finally {
      setScanning(false);
    }
  };

  const step = steps[currentStep];
  const Icon = step.icon;

  return (
    <div className="fixed inset-0 bg-white z-50 flex items-center justify-center">
      <div className="max-w-lg w-full mx-4">
        {/* Progress dots */}
        <div className="flex justify-center gap-2 mb-8">
          {steps.map((s, i) => (
            <div
              key={s.id}
              className={`w-2 h-2 rounded-full transition-colors ${
                i <= currentStep ? "bg-brand-500" : "bg-gray-200"
              }`}
            />
          ))}
        </div>

        {/* Step content */}
        <div className="text-center mb-8">
          <div className="w-16 h-16 rounded-full bg-brand-100 flex items-center justify-center mx-auto mb-4">
            <Icon className="w-8 h-8 text-brand-600" />
          </div>
          <h2 className="text-2xl font-bold text-gray-900 mb-2">{step.title}</h2>
          <p className="text-gray-500">{step.description}</p>
        </div>

        {/* Step actions */}
        <div className="flex justify-center">
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
            <div className="flex gap-3">
              <button
                onClick={handleAddFolder}
                className="flex items-center gap-2 px-6 py-3 bg-brand-600 text-white rounded-lg font-medium hover:bg-brand-700 transition-colors"
              >
                <FolderOpen className="w-4 h-4" />
                Choose Folder
              </button>
              <button
                onClick={() => setCurrentStep(2)}
                className="px-4 py-3 text-sm text-gray-500 hover:text-gray-700"
              >
                Skip for now
              </button>
            </div>
          )}

          {currentStep === 2 && (
            <div className="flex gap-3">
              <button
                onClick={handleFirstScan}
                disabled={scanning || !hasFolders}
                className="flex items-center gap-2 px-6 py-3 bg-brand-600 text-white rounded-lg font-medium hover:bg-brand-700 transition-colors disabled:opacity-50"
              >
                <RefreshCw className={`w-4 h-4 ${scanning ? "animate-spin" : ""}`} />
                {scanning ? "Scanning..." : hasFolders ? "Run First Scan" : "Add a folder first"}
              </button>
              <button
                onClick={() => setCurrentStep(3)}
                className="px-4 py-3 text-sm text-gray-500 hover:text-gray-700"
              >
                Skip
              </button>
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
                  <div key={text} className="flex items-center gap-2 text-sm text-gray-600">
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
