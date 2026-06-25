import { useEffect, useState } from "react";
import {
  AreaChart,
  Area,
  XAxis,
  YAxis,
  CartesianGrid,
  Tooltip,
  ResponsiveContainer,
} from "recharts";
import { getStorageSnapshots, formatBytes } from "../../lib/tauri";
import type { SnapshotRecord } from "../../lib/tauri";

export function StorageChart() {
  const [data, setData] = useState<SnapshotRecord[]>([]);
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    const load = async () => {
      try {
        // Use the first monitored folder or a default
        const snapshots = await getStorageSnapshots("*", 30);
        setData(snapshots);
      } catch {
        // No data yet
        setData([]);
      } finally {
        setLoading(false);
      }
    };
    load();
  }, []);

  if (loading) {
    return (
      <div className="flex items-center justify-center h-48 text-gray-400">
        Loading storage data...
      </div>
    );
  }

  if (data.length === 0) {
    return (
      <div className="flex flex-col items-center justify-center h-48 text-gray-400">
        <p>No storage data yet.</p>
        <p className="text-sm">Data appears after your first scan.</p>
      </div>
    );
  }

  const chartData = data.map((d) => ({
    date: new Date(d.snapshot_date).toLocaleDateString("en-US", { month: "short", day: "numeric" }),
    size: d.total_size,
    files: d.file_count,
  }));

  return (
    <ResponsiveContainer width="100%" height={200}>
      <AreaChart data={chartData}>
        <CartesianGrid strokeDasharray="3 3" stroke="#f0f0f0" />
        <XAxis
          dataKey="date"
          tick={{ fontSize: 12 }}
          stroke="#9ca3af"
        />
        <YAxis
          tickFormatter={(v) => formatBytes(v)}
          tick={{ fontSize: 12 }}
          stroke="#9ca3af"
        />
        <Tooltip
          formatter={(value: number) => [formatBytes(value), "Size"]}
          contentStyle={{
            borderRadius: "8px",
            border: "1px solid #e5e7eb",
            boxShadow: "0 2px 8px rgba(0,0,0,0.08)",
          }}
        />
        <Area
          type="monotone"
          dataKey="size"
          stroke="#6366f1"
          fill="#e0e7ff"
          strokeWidth={2}
        />
      </AreaChart>
    </ResponsiveContainer>
  );
}
