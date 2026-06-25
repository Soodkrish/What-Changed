import { useState, useEffect, useCallback, useRef } from "react";
import {
  getMonitoredFolders,
  getAllSettings,
  type MonitoredFolder,
} from "../lib/tauri";

export function useSettings() {
  const [folders, setFolders] = useState<MonitoredFolder[]>([]);
  const [settings, setSettings] = useState<Record<string, string>>({});
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const reqId = useRef(0);

  const refresh = useCallback(async (initial = false) => {
    const id = ++reqId.current;
    if (initial) setLoading(true);
    try {
      const [foldersData, settingsData] = await Promise.all([
        getMonitoredFolders(),
        getAllSettings(),
      ]);
      if (id !== reqId.current) return; // stale
      setFolders(foldersData);
      setSettings(settingsData);
      setError(null);
    } catch (err) {
      if (id !== reqId.current) return;
      setError(String(err));
    } finally {
      if (id === reqId.current) setLoading(false);
    }
  }, []);

  useEffect(() => {
    refresh(true);
  }, [refresh]);

  return { folders, settings, loading, error, refresh };
}
