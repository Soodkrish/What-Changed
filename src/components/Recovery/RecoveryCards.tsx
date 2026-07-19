import { RotateCcw, Camera, Cloud, Shield } from "lucide-react";
import type { RecoveryStats } from "../../lib/tauri";
import { formatBytes } from "../../lib/tauri";

interface RecoveryCardsProps {
  stats: RecoveryStats;
}

export function RecoveryCards({ stats }: RecoveryCardsProps) {
  const cards = [
    {
      label: "Recycle Bin",
      value: stats.recycle_bin_count,
      suffix: "files",
      icon: RotateCcw,
      color: "text-orange-600",
      bg: "bg-orange-50",
      border: "border-orange-200",
    },
    {
      label: "Snapshots",
      value: stats.snapshot_count,
      suffix: "versions",
      icon: Camera,
      color: "text-blue-600",
      bg: "bg-blue-50",
      border: "border-blue-200",
      showSize: true,
    },
    {
      label: "Cloud Synced",
      value: stats.cloud_folders_count,
      suffix: "folders",
      icon: Cloud,
      color: "text-cyan-600",
      bg: "bg-cyan-50",
      border: "border-cyan-200",
    },
    {
      label: "Total Protected",
      value: stats.recycle_bin_count + stats.snapshot_count,
      suffix: "items",
      icon: Shield,
      color: "text-emerald-600",
      bg: "bg-emerald-50",
      border: "border-emerald-200",
    },
  ];

  return (
    <div className="grid grid-cols-2 lg:grid-cols-4 gap-4">
      {cards.map((card) => (
        <div
          key={card.label}
          className={`bg-white dark:bg-gray-800 rounded-xl border ${card.border} p-5 flex items-center gap-4`}
        >
          <div className={`${card.bg} p-3 rounded-lg`}>
            <card.icon className={`w-6 h-6 ${card.color}`} />
          </div>
          <div>
            <p className="text-2xl font-bold text-gray-900 dark:text-white">{card.value}</p>
            {card.label === "Total Protected" && (
              <p className="text-xs text-gray-400 mt-0.5">
                {formatBytes(stats.total_snapshot_size)} stored
              </p>
            )}
            <p className="text-sm text-gray-500">
              {card.label}
              {card.showSize
                ? ` (${formatBytes(stats.total_snapshot_size)})`
                : ` ${card.suffix}`}
            </p>
          </div>
        </div>
      ))}
    </div>
  );
}
