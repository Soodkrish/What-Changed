import { useState, useEffect } from "react";
import { Calendar } from "lucide-react";
import type { HeatmapEntry } from "../../lib/tauri";
import { getActivityHeatmap, parseDbTimestamp } from "../../lib/tauri";

function getHeatColor(count: number): string {
  if (count === 0) return "bg-gray-100 dark:bg-gray-800";
  if (count <= 3) return "bg-emerald-200 dark:bg-emerald-900";
  if (count <= 7) return "bg-emerald-400 dark:bg-emerald-700";
  if (count <= 15) return "bg-emerald-600 dark:bg-emerald-500";
  return "bg-emerald-800 dark:bg-emerald-300";
}

function getDayOfWeek(dateStr: string): number {
  return new Date(dateStr).getDay();
}

function formatDate(dateStr: string): string {
  return parseDbTimestamp(dateStr).toLocaleDateString("en-US", {
    month: "short",
    day: "numeric",
  });
}

export function ActivityHeatmap() {
  const [entries, setEntries] = useState<HeatmapEntry[]>([]);
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    getActivityHeatmap(90)
      .then(setEntries)
      .catch(() => {})
      .finally(() => setLoading(false));
  }, []);

  // Build a map of date -> count
  const countMap = new Map<string, number>();
  for (const e of entries) {
    countMap.set(e.date, e.count);
  }

  // Generate last 90 days
  const days: { date: string; count: number }[] = [];
  const now = new Date();
  for (let i = 89; i >= 0; i--) {
    const d = new Date(now);
    d.setDate(d.getDate() - i);
    const key = d.toISOString().split("T")[0];
    days.push({ date: key, count: countMap.get(key) || 0 });
  }

  // Group by week (columns)
  const weeks: { date: string; count: number; dayOfWeek: number }[][] = [];
  let currentWeek: { date: string; count: number; dayOfWeek: number }[] = [];

  // Pad the first week with empty cells
  if (days.length > 0) {
    const firstDow = getDayOfWeek(days[0].date);
    for (let i = 0; i < firstDow; i++) {
      currentWeek.push({ date: "", count: -1, dayOfWeek: i });
    }
  }

  for (const day of days) {
    const dow = getDayOfWeek(day.date);
    currentWeek.push({ ...day, dayOfWeek: dow });
    if (dow === 6) {
      weeks.push(currentWeek);
      currentWeek = [];
    }
  }
  if (currentWeek.length > 0) {
    weeks.push(currentWeek);
  }

  const totalChanges = entries.reduce((sum, e) => sum + e.count, 0);
  const activeDays = entries.filter((e) => e.count > 0).length;
  const maxCount = Math.max(...entries.map((e) => e.count), 0);

  if (loading) {
    return (
      <div className="bg-white rounded-xl border border-gray-200 p-5 dark:bg-gray-900 dark:border-gray-700">
        <div className="animate-pulse h-32 bg-gray-100 dark:bg-gray-800 rounded" />
      </div>
    );
  }

  return (
    <div className="bg-white rounded-xl border border-gray-200 p-5 dark:bg-gray-900 dark:border-gray-700">
      <div className="flex items-center justify-between mb-4">
        <div className="flex items-center gap-3">
          <div className="w-10 h-10 rounded-full bg-emerald-100 dark:bg-emerald-900/30 flex items-center justify-center">
            <Calendar className="w-5 h-5 text-emerald-600 dark:text-emerald-400" />
          </div>
          <div>
            <h3 className="text-base font-bold text-gray-900 dark:text-white">Activity</h3>
            <p className="text-sm text-gray-500 dark:text-gray-400">
              {totalChanges} changes across {activeDays} days
            </p>
          </div>
        </div>
        {maxCount > 0 && (
          <div className="flex items-center gap-1.5 text-xs text-gray-500 dark:text-gray-400">
            <span>Less</span>
            {[0, 3, 7, 15].map((threshold) => (
              <div
                key={threshold}
                className={`w-3 h-3 rounded-sm ${
                  threshold === 0 ? "bg-gray-100 dark:bg-gray-800" :
                  threshold === 3 ? "bg-emerald-200 dark:bg-emerald-900" :
                  threshold === 7 ? "bg-emerald-400 dark:bg-emerald-700" :
                  "bg-emerald-600 dark:bg-emerald-500"
                }`}
              />
            ))}
            <span>More</span>
          </div>
        )}
      </div>

      {/* Heatmap grid */}
      <div className="overflow-x-auto">
        <div className="inline-flex gap-0.5">
          {weeks.map((week, wi) => (
            <div key={wi} className="flex flex-col gap-0.5">
              {week.map((day, di) => (
                <div
                  key={di}
                  className={`w-3 h-3 rounded-sm ${day.date === "" ? "bg-transparent" : getHeatColor(day.count)} transition-colors`}
                  title={day.date ? `${formatDate(day.date)}: ${day.count} changes` : ""}
                />
              ))}
            </div>
          ))}
        </div>
      </div>
    </div>
  );
}
