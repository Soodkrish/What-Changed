import { useState, useEffect, useCallback } from "react";
import {
  getDuplicateGroups,
  type DuplicateGroupRecord,
} from "../lib/tauri";

export function useDuplicates() {
  const [groups, setGroups] = useState<DuplicateGroupRecord[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  const refresh = useCallback(async () => {
    try {
      setLoading(true);
      const data = await getDuplicateGroups();
      setGroups(data);
      setError(null);
    } catch (err) {
      setError(String(err));
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    refresh();
  }, [refresh]);

  return { groups, loading, error, refresh };
}
