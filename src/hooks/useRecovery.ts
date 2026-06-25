import { useState, useEffect, useCallback } from "react";
import {
  getRecoverableFiles,
  getSnapshotStats,
  getCloudFolders,
  getRecoveryStats,
} from "../lib/tauri";
import type {
  RecycleBinEntry,
  CloudFolder,
  RecoveryStats,
} from "../lib/tauri";

export function useRecovery() {
  const [recycleBinFiles, setRecycleBinFiles] = useState<RecycleBinEntry[]>([]);
  const [snapshotStats, setSnapshotStats] = useState<[number, number]>([0, 0]);
  const [cloudFolders, setCloudFolders] = useState<CloudFolder[]>([]);
  const [recoveryStats, setRecoveryStats] = useState<RecoveryStats>({
    recycle_bin_count: 0,
    snapshot_count: 0,
    total_snapshot_size: 0,
    cloud_folders_count: 0,
  });
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  const refresh = useCallback(async () => {
    try {
      const [rb, ss, cf, rs] = await Promise.all([
        getRecoverableFiles(),
        getSnapshotStats(),
        getCloudFolders(),
        getRecoveryStats(),
      ]);
      setRecycleBinFiles(rb);
      setSnapshotStats(ss);
      setCloudFolders(cf);
      setRecoveryStats(rs);
      setError(null);
    } catch (err) {
      setError(err as string);
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    refresh();

    const interval = setInterval(() => {
      // Only poll when the app is visible (not minimized/backgrounded)
      if (document.visibilityState === "visible") {
        refresh();
      }
    }, 30000);

    // Also refresh immediately when app becomes visible again
    const handleVisibilityChange = () => {
      if (document.visibilityState === "visible") {
        refresh();
      }
    };
    document.addEventListener("visibilitychange", handleVisibilityChange);

    return () => {
      clearInterval(interval);
      document.removeEventListener("visibilitychange", handleVisibilityChange);
    };
  }, [refresh]);

  return {
    recycleBinFiles,
    snapshotStats,
    cloudFolders,
    recoveryStats,
    loading,
    error,
    refresh,
  };
}
