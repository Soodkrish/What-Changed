import { Cloud, RefreshCw, FolderSync, HardDrive, Box, Smartphone } from "lucide-react";
import type { CloudFolder } from "../../lib/tauri";
import { detectCloudFolders } from "../../lib/tauri";
import { useState } from "react";

interface CloudPanelProps {
  folders: CloudFolder[];
  onRefresh: () => void;
}

function getProviderInfo(provider: string) {
  const lower = provider.toLowerCase();
  if (lower.includes("onedrive")) {
    return {
      icon: FolderSync,
      label: "OneDrive",
      color: "text-blue-600",
      bg: "bg-blue-100",
      badge: "bg-blue-50 text-blue-700 border-blue-200",
    };
  }
  if (lower.includes("google")) {
    return {
      icon: HardDrive,
      label: "Google Drive",
      color: "text-yellow-600",
      bg: "bg-yellow-100",
      badge: "bg-yellow-50 text-yellow-700 border-yellow-200",
    };
  }
  if (lower.includes("dropbox")) {
    return {
      icon: Box,
      label: "Dropbox",
      color: "text-indigo-600",
      bg: "bg-indigo-100",
      badge: "bg-indigo-50 text-indigo-700 border-indigo-200",
    };
  }
  if (lower.includes("icloud")) {
    return {
      icon: Smartphone,
      label: "iCloud",
      color: "text-gray-600",
      bg: "bg-gray-100",
      badge: "bg-gray-50 text-gray-700 border-gray-200",
    };
  }
  return {
    icon: Cloud,
    label: provider,
    color: "text-cyan-600",
    bg: "bg-cyan-100",
    badge: "bg-cyan-50 text-cyan-700 border-cyan-200",
  };
}

export function CloudPanel({ folders, onRefresh }: CloudPanelProps) {
  const [detecting, setDetecting] = useState(false);

  const handleDetect = async () => {
    setDetecting(true);
    try {
      await detectCloudFolders();
      onRefresh();
    } catch (err) {
      console.error("Cloud detection failed:", err);
    } finally {
      setDetecting(false);
    }
  };

  return (
    <div className="bg-white rounded-xl border border-gray-200 overflow-hidden">
      <div className="flex items-center justify-between p-5 border-b border-gray-100">
        <div className="flex items-center gap-3">
          <div className="w-10 h-10 rounded-full bg-cyan-100 flex items-center justify-center">
            <Cloud className="w-5 h-5 text-cyan-600" />
          </div>
          <div>
            <h3 className="text-base font-bold text-gray-900">Cloud Synced</h3>
            <p className="text-sm text-gray-500">
              {folders.length} detected folder{folders.length !== 1 ? "s" : ""}
            </p>
          </div>
        </div>
        <button
          onClick={handleDetect}
          disabled={detecting}
          className="p-2 text-gray-400 hover:text-brand-500 hover:bg-gray-50 rounded-lg transition-colors disabled:opacity-50"
          title="Detect cloud folders"
        >
          <RefreshCw className={`w-4 h-4 ${detecting ? "animate-spin" : ""}`} />
        </button>
      </div>

      {folders.length === 0 ? (
        <div className="p-8 text-center text-gray-500">
          <Cloud className="w-8 h-8 mx-auto text-gray-300 mb-2" />
          <p className="text-sm">No cloud sync folders detected</p>
          <p className="text-xs text-gray-400 mt-1">
            Click refresh to scan for OneDrive, Google Drive, or Dropbox
          </p>
          <button
            onClick={handleDetect}
            disabled={detecting}
            className="mt-3 px-4 py-2 text-sm font-medium text-white bg-brand-500 rounded-lg hover:bg-brand-600 disabled:opacity-50 transition-colors"
          >
            {detecting ? "Scanning..." : "Scan Now"}
          </button>
        </div>
      ) : (
        <div className="divide-y divide-gray-50">
          {folders.map((folder) => {
            const info = getProviderInfo(folder.provider);
            const Icon = info.icon;
            return (
              <div
                key={folder.id}
                className="flex items-center gap-3 px-5 py-3 hover:bg-gray-50/50 transition-colors"
              >
                <div className={`w-8 h-8 rounded-lg ${info.bg} flex items-center justify-center flex-shrink-0`}>
                  <Icon className={`w-4 h-4 ${info.color}`} />
                </div>
                <div className="flex-1 min-w-0">
                  <p className="text-sm font-medium text-gray-900 truncate">
                    {folder.display_name || info.label}
                  </p>
                  <p className="text-xs text-gray-400 truncate" title={folder.path}>
                    ...{folder.path.split(/[/\\]/).slice(-2).join("/")}
                  </p>
                </div>
                <span
                  className={`px-2 py-0.5 text-xs font-medium rounded-full border ${info.badge}`}
                >
                  {info.label}
                </span>
                <span className="text-xs text-gray-400 flex-shrink-0">
                  {folder.is_active ? "Active" : "Inactive"}
                </span>
              </div>
            );
          })}
        </div>
      )}
    </div>
  );
}
