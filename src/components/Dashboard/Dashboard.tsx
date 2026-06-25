import { useState, useMemo } from "react";
import { useChanges } from "../../hooks/useChanges";
import { useScanBatches } from "../../hooks/useScanBatches";
import { StatsCards } from "./StatsCards";
import { FolderCard } from "../Changes/FolderCard";
import { StorageChart } from "./StorageChart";
import { ActivityHeatmap } from "./ActivityHeatmap";
import { FileTypeAnalytics } from "./FileTypeAnalytics";
import { Loader2, FolderOpen } from "lucide-react";

export function Dashboard() {
  const { changes, stats, loading, error } = useChanges();
  const { batches, loading: batchesLoading } = useScanBatches();
  const [activeFilter, setActiveFilter] = useState<string | null>(null);

  // Group batches by folder name from folders_scanned
  // NOTE: All hooks must be called before any early returns (React Rules of Hooks)
  const folderGroups = useMemo(() => {
    const map = new Map<string, {
      name: string;
      path: string;
      batches: typeof batches;
      allChanges: typeof batches[0]["changes"];
    }>();

    for (const batch of batches) {
      const folderNames = (batch.batch.folders_scanned || "Unknown").split(",").map((s) => s.trim());
      const primaryName = folderNames[0] || "Unknown";

      if (!map.has(primaryName)) {
        // Extract folder path from first change
        const firstPath = batch.changes[0]?.file_path || "";
        const folderPath = firstPath
          ? firstPath.replace(/\\/g, "/").split("/").slice(0, -1).join("/")
          : "";
        map.set(primaryName, {
          name: primaryName,
          path: folderPath,
          batches: [],
          allChanges: [],
        });
      }

      const group = map.get(primaryName)!;
      group.batches.push(batch);
      group.allChanges.push(...batch.changes);
    }

    // Sort by latest scan time descending
    return [...map.values()].sort((a, b) => {
      const aTime = a.batches[0]?.batch.started_at || "";
      const bTime = b.batches[0]?.batch.started_at || "";
      return bTime.localeCompare(aTime);
    });
  }, [batches]);

  // --- EARLY RETURNS BELOW (after all hooks are called) ---

  if (loading || batchesLoading) {
    return (
      <div className="flex items-center justify-center h-96">
        <Loader2 className="w-8 h-8 text-brand-500 animate-spin" />
      </div>
    );
  }

  if (error) {
    return (
      <div className="p-6">
        <div className="bg-red-50 border border-red-200 rounded-lg p-4 text-red-700">
          Error loading data: {error}
        </div>
      </div>
    );
  }

  return (
    <div className="space-y-6">
      <div>
        <h2 className="text-2xl font-bold text-gray-900">Dashboard</h2>
        <p className="text-sm text-gray-500 mt-1">
          {new Date().toLocaleDateString("en-US", {
            weekday: "long",
            year: "numeric",
            month: "long",
            day: "numeric",
          })}
        </p>
      </div>

      {/* Stats cards — clickable filters */}
      {stats && (
        <StatsCards
          stats={stats}
          onFilter={setActiveFilter}
          allChanges={changes}
        />
      )}

      {/* Activity Heatmap */}
      <ActivityHeatmap />

      {/* File Type Analytics */}
      <FileTypeAnalytics />

      <div className="grid grid-cols-1 lg:grid-cols-2 gap-6">
        {/* Folder cards — one per monitored folder */}
        <div className="space-y-4">
          {folderGroups.length === 0 ? (
            <div className="bg-white rounded-xl border border-gray-200 p-8 text-center text-gray-500">
              <FolderOpen className="w-8 h-8 mx-auto text-gray-300 mb-2" />
              <p>No folders scanned yet.</p>
              <p className="text-sm mt-1">Add a folder in Settings, then scan.</p>
            </div>
          ) : (
            folderGroups.map((group) => (
              <FolderCard
                key={group.name}
                folderName={group.name}
                folderPath={group.path}
                batches={group.batches}
                allChanges={group.allChanges}
                defaultExpanded={folderGroups.length === 1}
                filter={activeFilter}
              />
            ))
          )}
        </div>

        {/* Storage Growth */}
        <div className="bg-white rounded-xl border border-gray-200 p-6 h-fit">
          <h3 className="text-lg font-semibold text-gray-900 mb-4">Storage Growth</h3>
          <StorageChart />
        </div>
      </div>
    </div>
  );
}
