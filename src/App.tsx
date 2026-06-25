import { useState, useEffect, useCallback } from "react";
import { Layout } from "./components/Common/Layout";
import { Dashboard } from "./components/Dashboard/Dashboard";
import { Settings } from "./components/Settings/Settings";
import { DuplicatesView } from "./components/Duplicates/DuplicatesView";
import { RecoveryView } from "./components/Recovery/RecoveryView";
import { ChangelogGenerator } from "./components/Common/ChangelogGenerator";
import { ScanProgress } from "./components/Common/ScanProgress";
import { ErrorBoundary } from "./components/Common/ErrorBoundary";
import { OnboardingFlow } from "./components/Common/OnboardingFlow";
import { listen } from "@tauri-apps/api/event";
import { useDarkMode } from "./hooks/useDarkMode";
import { useKeyboardShortcuts } from "./hooks/useKeyboardShortcuts";
import { getMonitoredFolders, scanAll } from "./lib/tauri";

type ViewType = "dashboard" | "settings" | "duplicates" | "recovery" | "changelog";

const VALID_VIEWS: ViewType[] = ["dashboard", "settings", "duplicates", "recovery", "changelog"];

function App() {
  const [currentView, setCurrentView] = useState<ViewType>("dashboard");
  const [refreshKey, setRefreshKey] = useState(0);
  const { dark, toggle: toggleDark } = useDarkMode();
  const [showOnboarding, setShowOnboarding] = useState(false);
  const [onboardingChecked, setOnboardingChecked] = useState(false);
  const [closeToast, setCloseToast] = useState(false);

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
      await scanAll();
      setRefreshKey((prev) => prev + 1);
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

  useKeyboardShortcuts({
    onNavigate: setCurrentView,
    onScan: handleScan,
    onToggleDark: toggleDark,
  });

  return (
    <ErrorBoundary fallbackTitle="App Error" fallbackMessage="What Changed? encountered an unexpected error.">
      {showOnboarding && onboardingChecked && (
        <OnboardingFlow onComplete={() => setShowOnboarding(false)} />
      )}
      <Layout currentView={currentView} onNavigate={setCurrentView} dark={dark} onToggleDark={toggleDark}>
        {currentView === "dashboard" && (
          <ErrorBoundary fallbackTitle="Dashboard Error">
            <Dashboard key={refreshKey} />
          </ErrorBoundary>
        )}
        {currentView === "settings" && (
          <ErrorBoundary fallbackTitle="Settings Error">
            <Settings />
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
    </ErrorBoundary>
  );
}

export default App;
