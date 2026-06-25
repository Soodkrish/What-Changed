import { useState, useEffect, useCallback, useRef } from "react";
import {
  getChangesToday,
  getChangeStatsToday,
  type ChangeRecord,
  type ChangeStats,
} from "../lib/tauri";

export function useChanges() {
  const [changes, setChanges] = useState<ChangeRecord[]>([]);
  const [stats, setStats] = useState<ChangeStats | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const reqId = useRef(0);

  const refresh = useCallback(async (initial = false) => {
    const id = ++reqId.current;
    if (initial) setLoading(true);
    try {
      const [changesData, statsData] = await Promise.all([
        getChangesToday(),
        getChangeStatsToday(),
      ]);
      if (id !== reqId.current) return; // stale — newer request in flight
      setChanges(changesData);
      setStats(statsData);
      setError(null);
    } catch (err) {
      if (id !== reqId.current) return;
      setError(err instanceof Error ? err.message : String(err));
    } finally {
      if (id === reqId.current) setLoading(false);
    }
  }, []);

  useEffect(() => {
    refresh(true);
    const interval = setInterval(() => refresh(false), 30000);
    return () => clearInterval(interval);
  }, [refresh]);

  return { changes, stats, loading, error, refresh };
}
