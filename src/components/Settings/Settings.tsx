import { useState, useEffect, useRef, useMemo } from "react";
import { useSettings } from "../../hooks/useSettings";
import { FolderList } from "./FolderList";
import {
  addMonitoredFolder,
  setSetting,
  openFolderPicker,
  restartScheduler,
  enableAutostart,
  disableAutostart,
  enableFileSnapshots,
  disableFileSnapshots,
} from "../../lib/tauri";
import { FolderPlus, Save, RotateCcw, Check, AlertCircle, Power, Camera } from "lucide-react";
import { IgnorePatterns } from "./IgnorePatterns";
import { NotificationProfiles } from "./NotificationProfiles";
import { WebhookSettings } from "./WebhookSettings";

interface SettingsProps {
  onDirtyChange?: (dirty: boolean) => void;
  onSavedFlash?: () => void;
}

export function Settings({ onDirtyChange, onSavedFlash }: SettingsProps) {
  const { folders, settings, loading, refresh } = useSettings();
  const [scanFrequency, setScanFrequency] = useState(
    settings.scan_frequency || "15"
  );
  const [startMinimized, setStartMinimized] = useState(
    settings.start_minimized === "true"
  );
  const [notificationsEnabled, setNotificationsEnabled] = useState(
    settings.notifications_enabled !== "false"
  );
  const [dailySummary, setDailySummary] = useState(
    settings.daily_summary_enabled !== "false"
  );
  const [dailySummaryWebhook, setDailySummaryWebhook] = useState(
    settings.daily_summary_webhook_enabled === "true"
  );
  const [dailySummaryTime, setDailySummaryTime] = useState(
    settings.daily_summary_time || "18:00"
  );
  const [autoStart, setAutoStart] = useState(
    settings.autostart_enabled === "true"
  );
  const [snapshotsEnabled, setSnapshotsEnabled] = useState(
    settings.file_snapshots_enabled === "true"
  );
  const [saving, setSaving] = useState(false);
  const [toast, setToast] = useState<{ type: "success" | "error"; message: string } | null>(null);
  const [savedFlash, setSavedFlash] = useState(false);
  const toastTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const savedFlashTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  // Snapshot of the last saved values for dirty detection
  const lastSavedRef = useRef<string>("");

  // Build a fingerprint of current settings values
  const currentFingerprint = useMemo(() =>
    JSON.stringify({
      scanFrequency,
      startMinimized,
      notificationsEnabled,
      dailySummary,
      dailySummaryWebhook,
      dailySummaryTime,
      autoStart,
      snapshotsEnabled,
    }),
    [scanFrequency, startMinimized, notificationsEnabled, dailySummary, dailySummaryWebhook, dailySummaryTime, autoStart, snapshotsEnabled]
  );

  // Track dirty state
  const isDirty = currentFingerprint !== lastSavedRef.current && lastSavedRef.current !== "";

  // Notify parent of dirty state changes
  useEffect(() => {
    onDirtyChange?.(isDirty);
    return () => onDirtyChange?.(false);
  }, [isDirty, onDirtyChange]);

  // Clear timers on unmount
  useEffect(() => {
    return () => {
      if (toastTimerRef.current) clearTimeout(toastTimerRef.current);
      if (savedFlashTimerRef.current) clearTimeout(savedFlashTimerRef.current);
    };
  }, []);

  // Sync local state when settings load (fixes stale initialization)
  useEffect(() => {
    if (settings.scan_frequency) setScanFrequency(settings.scan_frequency);
    if (settings.start_minimized !== undefined) setStartMinimized(settings.start_minimized === "true");
    if (settings.notifications_enabled !== undefined) setNotificationsEnabled(settings.notifications_enabled !== "false");
    if (settings.daily_summary_enabled !== undefined) setDailySummary(settings.daily_summary_enabled !== "false");
    if (settings.daily_summary_webhook_enabled !== undefined) setDailySummaryWebhook(settings.daily_summary_webhook_enabled === "true");
    if (settings.daily_summary_time !== undefined) setDailySummaryTime(settings.daily_summary_time);
    if (settings.autostart_enabled !== undefined) setAutoStart(settings.autostart_enabled === "true");
    if (settings.file_snapshots_enabled !== undefined) setSnapshotsEnabled(settings.file_snapshots_enabled === "true");
    // Update the saved snapshot after settings load
    if (lastSavedRef.current === "") {
      lastSavedRef.current = JSON.stringify({
        scanFrequency: settings.scan_frequency || "15",
        startMinimized: settings.start_minimized === "true",
        notificationsEnabled: settings.notifications_enabled !== "false",
        dailySummary: settings.daily_summary_enabled !== "false",
        dailySummaryWebhook: settings.daily_summary_webhook_enabled === "true",
        dailySummaryTime: settings.daily_summary_time || "18:00",
        autoStart: settings.autostart_enabled === "true",
        snapshotsEnabled: settings.file_snapshots_enabled === "true",
      });
    }
  }, [settings]);

  const showToast = (type: "success" | "error", message: string) => {
    if (toastTimerRef.current) clearTimeout(toastTimerRef.current);
    setToast({ type, message });
    toastTimerRef.current = setTimeout(() => {
      setToast(null);
      toastTimerRef.current = null;
    }, 3000);
  };

  const handleAddFolder = async () => {
    try {
      const path = await openFolderPicker();
      if (path) {
        // Check for duplicates
        if (folders.some(f => f.path === path)) {
          showToast("error", "This folder is already being monitored");
          return;
        }
        await addMonitoredFolder(path);
        refresh();
        showToast("success", `Now monitoring: ${path.split(/[/\\]/).pop()}`);
      }
    } catch (err: any) {
      const msg = typeof err === "string" ? err : err?.message || "Failed to add folder";
      showToast("error", `Could not add folder: ${msg}`);
    }
  };

  const handleSave = async () => {
    setSaving(true);
    try {
      await Promise.all([
        setSetting("scan_frequency", scanFrequency),
        setSetting("start_minimized", String(startMinimized)),
        setSetting("notifications_enabled", String(notificationsEnabled)),
        setSetting("daily_summary_enabled", String(dailySummary)),
        setSetting("daily_summary_webhook_enabled", String(dailySummaryWebhook)),
        setSetting("daily_summary_time", dailySummaryTime),
        setSetting("autostart_enabled", String(autoStart)),
        setSetting("file_snapshots_enabled", String(snapshotsEnabled)),
      ]);

      // Handle file snapshots
      if (snapshotsEnabled) {
        await enableFileSnapshots();
      } else {
        await disableFileSnapshots();
      }

      // Handle auto-start
      if (autoStart) {
        await enableAutostart(startMinimized);
      } else {
        await disableAutostart();
      }

      // Restart scheduler with new frequency
      await restartScheduler();

      // Update the saved snapshot so dirty detection resets
      lastSavedRef.current = currentFingerprint;

      // Show green flash for 2 seconds
      setSavedFlash(true);
      onSavedFlash?.();
      if (savedFlashTimerRef.current) clearTimeout(savedFlashTimerRef.current);
      savedFlashTimerRef.current = setTimeout(() => setSavedFlash(false), 2000);

      showToast("success", "Settings saved successfully");
    } catch (err) {
      showToast("error", `Failed to save settings: ${err}`);
    } finally {
      setSaving(false);
    }
  };

  const handleReset = () => {
    setScanFrequency("15");
    setStartMinimized(false);
    setNotificationsEnabled(true);
    setDailySummary(true);
    setDailySummaryWebhook(false);
    setDailySummaryTime("18:00");
    setAutoStart(false);
    setSnapshotsEnabled(false);
    // Update snapshot so reset itself isn't "dirty"
    lastSavedRef.current = JSON.stringify({
      scanFrequency: "15",
      startMinimized: false,
      notificationsEnabled: true,
      dailySummary: true,
      dailySummaryWebhook: false,
      dailySummaryTime: "18:00",
      autoStart: false,
      snapshotsEnabled: false,
    });
    showToast("success", "Settings reset to defaults");
  };

  const handleFolderRemoved = (folderName: string) => {
    refresh();
    showToast("success", `Removed: ${folderName}`);
  };

  if (loading) {
    return (
      <div className="flex items-center justify-center h-96 text-gray-400">
        Loading settings...
      </div>
    );
  }

  // Ribbon color: green flash when just saved, red when dirty, default otherwise
  const ribbonColor = savedFlash
    ? "bg-emerald-500"
    : isDirty
    ? "bg-red-500"
    : "bg-gray-300 dark:bg-gray-600";

  const ribbonLabel = savedFlash
    ? "Saved"
    : isDirty
    ? "Unsaved changes"
    : "";

  return (
    <div className="space-y-6 max-w-2xl">
      <div>
        <div className="flex items-center gap-3">
          <h2 className="text-2xl font-bold text-gray-900 dark:text-white">Settings</h2>
          {(isDirty || savedFlash) && (
            <span className={`inline-flex items-center gap-1.5 px-2.5 py-0.5 rounded-full text-xs font-medium text-white transition-colors ${ribbonColor}`}>
              {savedFlash ? (
                <Check className="w-3 h-3" />
              ) : (
                <AlertCircle className="w-3 h-3" />
              )}
              {ribbonLabel}
            </span>
          )}
        </div>
        <p className="text-sm text-gray-500 mt-1">
          Configure what and how to monitor.
        </p>
        {/* Subtle ribbon line */}
        <div className={`h-0.5 rounded-full mt-3 transition-colors ${ribbonColor}`} />
      </div>

      {/* Monitored Folders */}
      <div className="bg-white dark:bg-gray-800 rounded-xl border border-gray-200 dark:border-gray-700 p-6">
        <div className="flex items-center justify-between mb-4">
          <h3 className="text-lg font-semibold text-gray-900 dark:text-white">Monitored Folders</h3>
          <button
            onClick={handleAddFolder}
            className="flex items-center gap-2 px-3 py-1.5 bg-brand-600 text-white rounded-lg text-sm font-medium hover:bg-brand-700 transition-colors"
          >
            <FolderPlus className="w-4 h-4" />
            Add Folder
          </button>
        </div>
        <FolderList folders={folders} onRefresh={refresh} onFolderRemoved={handleFolderRemoved} />
      </div>

      {/* Scan Frequency */}
      <div className="bg-white dark:bg-gray-800 rounded-xl border border-gray-200 dark:border-gray-700 p-6">
        <h3 className="text-lg font-semibold text-gray-900 dark:text-white mb-4">Scan Frequency</h3>
        <div className="space-y-2">
          {[
            { value: "5", label: "Every 5 minutes" },
            { value: "15", label: "Every 15 minutes (recommended)" },
            { value: "60", label: "Every hour" },
            { value: "1440", label: "Once daily" },
          ].map((option) => (
            <label
              key={option.value}
              className="flex items-center gap-3 p-3 rounded-lg hover:bg-gray-50 dark:hover:bg-gray-700 cursor-pointer"
            >
              <input
                type="radio"
                name="scan_frequency"
                value={option.value}
                checked={scanFrequency === option.value}
                onChange={(e) => setScanFrequency(e.target.value)}
                className="w-4 h-4 text-brand-600 focus:ring-brand-500"
              />
              <span className="text-sm text-gray-700 dark:text-gray-300">{option.label}</span>
            </label>
          ))}
        </div>
      </div>

      {/* Startup & Notifications */}
      <div className="bg-white dark:bg-gray-800 rounded-xl border border-gray-200 dark:border-gray-700 p-6">
        <h3 className="text-lg font-semibold text-gray-900 dark:text-white mb-4">Startup & Notifications</h3>
        <div className="space-y-4">
          <label className="flex items-center gap-3 cursor-pointer">
            <input
              type="checkbox"
              checked={startMinimized}
              onChange={(e) => setStartMinimized(e.target.checked)}
              className="w-4 h-4 text-brand-600 rounded focus:ring-brand-500"
            />
            <div>
              <p className="text-sm font-medium text-gray-700 dark:text-gray-300">Start minimized to tray</p>
              <p className="text-xs text-gray-500">App starts in system tray on boot</p>
            </div>
          </label>

          <label className="flex items-center gap-3 cursor-pointer">
            <input
              type="checkbox"
              checked={notificationsEnabled}
              onChange={(e) => setNotificationsEnabled(e.target.checked)}
              className="w-4 h-4 text-brand-600 rounded focus:ring-brand-500"
            />
            <div>
              <p className="text-sm font-medium text-gray-700 dark:text-gray-300">Enable notifications</p>
              <p className="text-xs text-gray-500">Show desktop notifications for changes</p>
            </div>
          </label>

          <label className="flex items-center gap-3 cursor-pointer">
            <input
              type="checkbox"
              checked={dailySummary}
              onChange={(e) => setDailySummary(e.target.checked)}
              className="w-4 h-4 text-brand-600 rounded focus:ring-brand-500"
            />
            <div>
              <p className="text-sm font-medium text-gray-700 dark:text-gray-300">Daily summary</p>
              <p className="text-xs text-gray-500">Get a daily digest of all changes</p>
            </div>
          </label>

          <div className="border-t border-gray-100 dark:border-gray-700 pt-4 mt-2">
            <label className="flex items-center gap-3 cursor-pointer">
              <input
                type="checkbox"
                checked={dailySummaryWebhook}
                onChange={(e) => setDailySummaryWebhook(e.target.checked)}
                className="w-4 h-4 text-brand-600 rounded focus:ring-brand-500"
              />
              <div>
                <p className="text-sm font-medium text-gray-700 dark:text-gray-300">Send daily summary to webhooks</p>
                <p className="text-xs text-gray-500">Push a daily report to Discord / Telegram at a scheduled time</p>
              </div>
            </label>
            {dailySummaryWebhook && (
              <div className="mt-3 ml-7 flex items-center gap-3">
                <label className="text-sm text-gray-600 dark:text-gray-400">Send at:</label>
                <input
                  type="time"
                  value={dailySummaryTime}
                  onChange={(e) => setDailySummaryTime(e.target.value)}
                  className="px-3 py-1.5 text-sm border border-gray-200 dark:border-gray-600 rounded-lg focus:outline-none focus:ring-2 focus:ring-brand-500 bg-white dark:bg-gray-900 dark:text-white"
                />
                <span className="text-xs text-gray-400">once per day</span>
              </div>
            )}
          </div>

          <div className="border-t border-gray-100 dark:border-gray-700 pt-4 mt-4">
            <label className="flex items-center gap-3 cursor-pointer">
              <input
                type="checkbox"
                checked={autoStart}
                onChange={(e) => setAutoStart(e.target.checked)}
                className="w-4 h-4 text-brand-600 rounded focus:ring-brand-500"
              />
              <div className="flex items-center gap-2">
                <Power className="w-4 h-4 text-gray-400" />
                <div>
                  <p className="text-sm font-medium text-gray-700 dark:text-gray-300">Start on boot</p>
                  <p className="text-xs text-gray-500">Launch What Changed? when you log in</p>
                </div>
              </div>
            </label>
          </div>
        </div>
      </div>

      {/* Recovery & Snapshots */}
      <div className="bg-white dark:bg-gray-800 rounded-xl border border-gray-200 dark:border-gray-700 p-6">
        <div className="flex items-center gap-3 mb-4">
          <Camera className="w-5 h-5 text-blue-500" />
          <h3 className="text-lg font-semibold text-gray-900 dark:text-white">Recovery & Snapshots</h3>
        </div>
        <div className="space-y-4">
          <label className="flex items-center gap-3 cursor-pointer">
            <input
              type="checkbox"
              checked={snapshotsEnabled}
              onChange={(e) => setSnapshotsEnabled(e.target.checked)}
              className="w-4 h-4 text-brand-600 rounded focus:ring-brand-500"
            />
            <div>
              <p className="text-sm font-medium text-gray-700 dark:text-gray-300">Enable file snapshots</p>
              <p className="text-xs text-gray-500">
                Automatically back up text and code files before they change (compressed with zstd, up to 100KB per file)
              </p>
            </div>
          </label>
        </div>
      </div>

      {/* Ignore Patterns */}
      <IgnorePatterns folders={folders} />

      {/* Notification Profiles */}
      <NotificationProfiles />

      {/* Webhook Endpoints */}
      <WebhookSettings />

      {/* Actions */}
      <div className="flex items-center gap-3">
        <button
          onClick={handleSave}
          disabled={saving}
          className="flex items-center gap-2 px-4 py-2.5 bg-brand-600 text-white rounded-lg text-sm font-medium hover:bg-brand-700 transition-colors disabled:opacity-50"
        >
          <Save className="w-4 h-4" />
          {saving ? "Saving..." : "Save Settings"}
        </button>
        <button
          onClick={handleReset}
          className="flex items-center gap-2 px-4 py-2.5 bg-gray-100 text-gray-700 rounded-lg text-sm font-medium hover:bg-gray-200 transition-colors"
        >
          <RotateCcw className="w-4 h-4" />
          Reset to Defaults
        </button>
      </div>

      {/* Toast Notification */}
      {toast && (
        <div
          className={`fixed bottom-6 right-6 flex items-center gap-3 px-4 py-3 rounded-lg shadow-lg text-white text-sm font-medium ${
            toast.type === "success" ? "bg-emerald-600" : "bg-red-600"
          }`}
        >
          {toast.type === "success" ? (
            <Check className="w-4 h-4" />
          ) : (
            <AlertCircle className="w-4 h-4" />
          )}
          {toast.message}
        </div>
      )}
    </div>
  );
}
