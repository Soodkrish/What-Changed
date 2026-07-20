import { useState, useEffect, useCallback, lazy, Suspense, useRef } from "react";
import { Layout } from "./components/Common/Layout";
import { Dashboard } from "./components/Dashboard/Dashboard";
import { ScanProgress } from "./components/Common/ScanProgress";
import { ErrorBoundary } from "./components/Common/ErrorBoundary";
import { OnboardingFlow } from "./components/Common/OnboardingFlow";
import { UpdateBanner } from "./components/Common/UpdateBanner";
import { listen } from "@tauri-apps/api/event";
import { useDarkMode } from "./hooks/useDarkMode";
import { useKeyboardShortcuts } from "./hooks/useKeyboardShortcuts";
import { getMonitoredFolders, scanAllAsync, checkForUpdates, type UpdateInfo } from "./lib/tauri";
import { getCurrentWindow } from "@tauri-apps/api/window";

// Lazy-load non-default views (code splitting — saves 10-15MB upfront)
const Settings = lazy(() => import("./components/Settings/Settings").then(m => ({ default: m.Settings })));
const DuplicatesView = lazy(() => import("./components/Duplicates/DuplicatesView").then(m => ({ default: m.DuplicatesView })));
const RecoveryView = lazy(() => import("./components/Recovery/RecoveryView").then(m => ({ default: m.RecoveryView })));
const ChangelogGenerator = lazy(() => import("./components/Common/ChangelogGenerator").then(m => ({ default: m.ChangelogGenerator })));

type ViewType = "dashboard" | "settings" | "duplicates" | "recovery" | "changelog";

const VALID_VIEWS: ViewType[] = ["dashboard", "settings", "duplicates", "recovery", "changelog"];

function App() {
  const [currentView, setCurrentView] = useState<ViewType>("dashboard");
  const [refreshKey, setRefreshKey] = useState(0);
  const { dark, toggle: toggleDark } = useDarkMode();
  const [showOnboarding, setShowOnboarding] = useState(false);
  const [onboardingChecked, setOnboardingChecked] = useState(false);
  const [closeToast, setCloseToast] = useState(false);
  const [updateInfo, setUpdateInfo] = useState<UpdateInfo | null>(null);
  const [updateDismissed, setUpdateDismissed] = useState(false);
  const [hasUnsavedSettings, setHasUnsavedSettings] = useState(false);
  const [savedFlash, setSavedFlash] = useState(false);
  const [showUnsavedDialog, setShowUnsavedDialog] = useState(false);
  const pendingNavigation = useRef<ViewType | null>(null);
  const savedFlashTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  // Ribbon color: green flash when just saved, red when dirty, neutral otherwise
  const ribbonState = savedFlash ? "saved" : hasUnsavedSettings ? "dirty" : "clean";

  // Update window title bar with colored dot indicator
  useEffect(() => {
    const appWindow = getCurrentWindow();
    const dot = ribbonState === "saved" ? " \u{1F7E2}"  // 🟢 green
      : ribbonState === "dirty" ? " \u{1F534}"           // 🔴 red
      : "";
    appWindow.setTitle(`What Changed?${dot}`).catch(() => {});
  }, [ribbonState]);

  const handleSavedFlash = useCallback(() => {
    setSavedFlash(true);
    if (savedFlashTimerRef.current) clearTimeout(savedFlashTimerRef.current);
    savedFlashTimerRef.current = setTimeout(() => setSavedFlash(false), 2000);
  }, []);

  // Cleanup flash timer on unmount
  useEffect(() => {
    return () => {
      if (savedFlashTimerRef.current) clearTimeout(savedFlashTimerRef.current);
    };
  }, []);

  // Check if this is a first run
  useEffect(() => {
    getMonitoredFolders()
      .then((folders) => {
        if (folders.length === 0) {
          setShowOnboarding(true);
        }
        setOnboardingChecked(true);
      })
      .catch(() => {
        setOnboardingChecked(true);
      });
  }, []);

  // Check for updates on startup (once per session)
  useEffect(() => {
    checkForUpdates()
      .then((info) => {
        if (info.has_update) {
          setUpdateInfo(info);
        }
      })
      .catch(() => {}); // silently fail if offline or GitHub unreachable
  }, []);

  useEffect(() => {
    const unlisten = listen<string>("navigate", (event) => {
      const view = event.payload as ViewType;
      if (view && VALID_VIEWS.includes(view)) setCurrentView(view);
    });
    return () => { unlisten.then((fn) => fn()); };
  }, []);

  // Listen for close-warning (app hidden to tray, scans keep running)
  useEffect(() => {
    let timer: ReturnType<typeof setTimeout> | null = null;
    const unlisten = listen<void>("close-warning", () => {
      setCloseToast(true);
      timer = setTimeout(() => setCloseToast(false), 4000);
    });
    return () => {
      unlisten.then((fn) => fn());
      if (timer) clearTimeout(timer);
    };
  }, []);

  const handleScanComplete = useCallback(() => {
    setRefreshKey((prev) => prev + 1);
    setCurrentView("dashboard");
  }, []);

  const handleScan = useCallback(async () => {
    try {
      await scanAllAsync();
      // Progress and completion are handled by the ScanProgress listener
    } catch (err) {
      console.error("Scan failed:", err);
    }
  }, []);

  // Listen for tray "Scan Now" trigger
  useEffect(() => {
    const unlisten = listen<void>("trigger-scan", () => {
      handleScan();
    });
    return () => { unlisten.then((fn) => fn()); };
  }, [handleScan]);

  // Safe navigation: intercept navigation away from Settings if unsaved changes
  const safeNavigate = useCallback((view: ViewType) => {
    if (hasUnsavedSettings && currentView === "settings") {
      pendingNavigation.current = view;
      setShowUnsavedDialog(true);
    } else {
      setCurrentView(view);
    }
  }, [hasUnsavedSettings, currentView]);

  const handleUnsavedDiscard = useCallback(() => {
    setShowUnsavedDialog(false);
    setHasUnsavedSettings(false);
    if (pendingNavigation.current) {
      setCurrentView(pendingNavigation.current);
      pendingNavigation.current = null;
    }
  }, []);

  const handleUnsavedCancel = useCallback(() => {
    setShowUnsavedDialog(false);
    pendingNavigation.current = null;
  }, []);

  useKeyboardShortcuts({
    onNavigate: safeNavigate,
    onScan: handleScan,
    onToggleDark: toggleDark,
  });

  return (
    <ErrorBoundary fallbackTitle="App Error" fallbackMessage="What Changed? encountered an unexpected error.">
      {showOnboarding && onboardingChecked && (
        <OnboardingFlow onComplete={() => setShowOnboarding(false)} />
      )}
      <Layout currentView={currentView} onNavigate={safeNavigate} dark={dark} onToggleDark={toggleDark}>
        {/* Update notification banner */}
        {updateInfo && !updateDismissed && (
          <UpdateBanner
            updateInfo={updateInfo}
            onDismiss={() => setUpdateDismissed(true)}
          />
        )}
        {currentView === "dashboard" && (
          <ErrorBoundary fallbackTitle="Dashboard Error">
            <Dashboard key={refreshKey} />
          </ErrorBoundary>
        )}
        <Suspense fallback={
          <div className="flex items-center justify-center h-64">
            <div className="w-6 h-6 border-2 border-brand-500 border-t-transparent rounded-full animate-spin" />
          </div>
        }>
          {currentView === "settings" && (
            <ErrorBoundary fallbackTitle="Settings Error">
              <Settings onDirtyChange={setHasUnsavedSettings} onSavedFlash={handleSavedFlash} />
            </ErrorBoundary>
          )}
          {currentView === "duplicates" && (
            <ErrorBoundary fallbackTitle="Duplicates Error">
              <DuplicatesView key={refreshKey} />
            </ErrorBoundary>
          )}
          {currentView === "recovery" && (
            <ErrorBoundary fallbackTitle="Recovery Error">
              <RecoveryView />
            </ErrorBoundary>
          )}
          {currentView === "changelog" && (
            <ErrorBoundary fallbackTitle="Changelog Error">
              <ChangelogGenerator />
            </ErrorBoundary>
          )}
        </Suspense>
        <ScanProgress onComplete={handleScanComplete} />
      </Layout>

      {/* Close-to-tray warning toast */}
      {closeToast && (
        <div className="fixed bottom-6 right-6 flex items-center gap-3 px-4 py-3 rounded-lg shadow-lg bg-blue-600 text-white text-sm font-medium z-50">
          <svg className="w-4 h-4 flex-shrink-0" fill="none" viewBox="0 0 24 24" stroke="currentColor">
            <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M13 16h-1v-4h-1m1-4h.01M21 12a9 9 0 11-18 0 9 9 0 0118 0z" />
          </svg>
          Hidden to tray. Auto-scans continue in background.
        </div>
      )}

      {/* Unsaved settings confirmation dialog */}
      {showUnsavedDialog && (
        <div className="fixed inset-0 bg-black/50 flex items-center justify-center z-50">
          <div className="bg-white dark:bg-gray-800 rounded-xl shadow-2xl p-6 max-w-sm w-full mx-4">
            <h3 className="text-lg font-semibold text-gray-900 dark:text-white mb-2">
              Unsaved Changes
            </h3>
            <p className="text-sm text-gray-500 dark:text-gray-400 mb-6">
              You have unsaved changes in Settings. If you leave now, your changes will be lost.
            </p>
            <div className="flex items-center gap-3 justify-end">
              <button
                onClick={handleUnsavedCancel}
                className="px-4 py-2 text-sm font-medium text-gray-700 dark:text-gray-300 hover:bg-gray-100 dark:hover:bg-gray-700 rounded-lg transition-colors"
              >
                Stay on Settings
              </button>
              <button
                onClick={handleUnsavedDiscard}
                className="px-4 py-2 text-sm font-medium text-white bg-red-600 hover:bg-red-700 rounded-lg transition-colors"
              >
                Discard & Leave
              </button>
            </div>
          </div>
        </div>
      )}
    </ErrorBoundary>
  );
}

export default App;
