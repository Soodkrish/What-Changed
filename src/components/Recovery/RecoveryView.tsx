import { useRecovery } from "../../hooks/useRecovery";
import { RecoveryCards } from "./RecoveryCards";
import { RecycleBinPanel } from "./RecycleBinPanel";
import { SnapshotPanel } from "./SnapshotPanel";
import { CloudPanel } from "./CloudPanel";
import { ExportPanel } from "./ExportPanel";
import { Shield, Loader2 } from "lucide-react";

export function RecoveryView() {
  const {
    recycleBinFiles,
    snapshotStats,
    cloudFolders,
    recoveryStats,
    loading,
    error,
    refresh,
  } = useRecovery();

  if (loading) {
    return (
      <div className="flex items-center justify-center h-64">
        <Loader2 className="w-8 h-8 text-brand-500 animate-spin" />
        <span className="ml-3 text-gray-500">Loading recovery data...</span>
      </div>
    );
  }

  if (error) {
    return (
      <div className="flex flex-col items-center justify-center h-64 gap-3">
        <Shield className="w-12 h-12 text-red-300" />
        <p className="text-sm text-red-600">Failed to load recovery data</p>
        <button
          onClick={refresh}
          className="px-4 py-2 text-sm font-medium text-white bg-brand-500 rounded-lg hover:bg-brand-600 transition-colors"
        >
          Retry
        </button>
      </div>
    );
  }

  return (
    <div className="space-y-6 p-6 max-w-6xl mx-auto">
      {/* Header */}
      <div>
        <h1 className="text-2xl font-bold text-gray-900 dark:text-white">Recovery</h1>
        <p className="text-sm text-gray-500 mt-1">
          Safety net for your files — restore deleted items, roll back changes, and track cloud protection
        </p>
      </div>

      {/* Stats Cards */}
      <RecoveryCards stats={recoveryStats} />

      {/* Panels */}
      <div className="space-y-4">
        <RecycleBinPanel entries={recycleBinFiles} onRefresh={refresh} />
        <SnapshotPanel
          snapshotCount={snapshotStats[0]}
          totalSize={snapshotStats[1]}
        />
        <CloudPanel folders={cloudFolders} onRefresh={refresh} />
        <ExportPanel />
      </div>
    </div>
  );
}
