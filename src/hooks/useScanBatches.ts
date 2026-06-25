import { useState, useEffect, useCallback, useRef } from "react";
import {
  getScanBatches,
  type ScanBatchWithChanges,
} from "../lib/tauri";

export function useScanBatches() {
  const [batches, setBatches] = useState<ScanBatchWithChanges[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const reqId = useRef(0);

  const refresh = useCallback(async (initial = false) => {
    const id = ++reqId.current;
    if (initial) setLoading(true);
    try {
      const data = await getScanBatches();
      if (id !== reqId.current) return;
      setBatches(data);
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

  return { batches, loading, error, refresh };
}
