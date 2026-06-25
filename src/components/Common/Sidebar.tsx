import { LayoutDashboard, Settings, Copy, Shield, RefreshCw, Sun, Moon, FileText } from "lucide-react";
import { scanAll } from "../../lib/tauri";
import { useState } from "react";
import { ProfileSwitcher } from "./ProfileSwitcher";
import { SearchBar } from "./SearchBar";
import { ShortcutsHelp } from "./ShortcutsHelp";

interface SidebarProps {
  currentView: string;
  onNavigate: (view: "dashboard" | "settings" | "duplicates" | "recovery" | "changelog") => void;
  dark: boolean;
  onToggleDark: () => void;
}

const navItems = [
  { id: "dashboard" as const, label: "Dashboard", icon: LayoutDashboard },
  { id: "duplicates" as const, label: "Duplicates", icon: Copy },
  { id: "recovery" as const, label: "Recovery", icon: Shield },
  { id: "changelog" as const, label: "Changelog", icon: FileText },
  { id: "settings" as const, label: "Settings", icon: Settings },
];

export function Sidebar({ currentView, onNavigate, dark, onToggleDark }: SidebarProps) {
  const [scanning, setScanning] = useState(false);

  const handleScan = async () => {
    setScanning(true);
    try {
      await scanAll();
    } catch (err) {
      console.error("Scan failed:", err);
    } finally {
      setScanning(false);
    }
  };

  return (
    <aside className="w-64 bg-white border-r border-gray-200 dark:bg-gray-900 dark:border-gray-700 flex flex-col">
      <div className="p-6 border-b border-gray-200 dark:border-gray-700">
        <div className="flex items-center justify-between">
          <h1 className="text-xl font-bold text-brand-600 flex items-center gap-2">
            <span className="text-2xl">📂</span>
            What Changed?
          </h1>
          <div className="flex items-center gap-1">
            <ShortcutsHelp />
            <button
              onClick={onToggleDark}
              className="p-2 text-gray-400 hover:text-brand-500 hover:bg-gray-100 dark:hover:bg-gray-800 rounded-lg transition-colors"
              title={dark ? "Switch to light mode" : "Switch to dark mode"}
            >
              {dark ? <Sun className="w-4 h-4" /> : <Moon className="w-4 h-4" />}
            </button>
          </div>
        </div>
        <p className="text-xs text-gray-500 dark:text-gray-400 mt-1">File system monitor</p>
      </div>

      <div className="px-4 pt-4">
        <SearchBar />
      </div>

      <nav className="flex-1 p-4 space-y-1">
        {navItems.map((item) => (
          <button
            key={item.id}
            onClick={() => onNavigate(item.id)}
            className={`w-full flex items-center gap-3 px-3 py-2.5 rounded-lg text-sm font-medium transition-colors ${
              currentView === item.id
                ? "bg-brand-50 text-brand-700 dark:bg-brand-900/20 dark:text-brand-300"
                : "text-gray-600 hover:bg-gray-100 hover:text-gray-900 dark:text-gray-400 dark:hover:bg-gray-800 dark:hover:text-white"
            }`}
          >
            <item.icon className="w-5 h-5" />
            {item.label}
          </button>
        ))}
      </nav>

      <ProfileSwitcher />

      <div className="p-4 border-t border-gray-200 dark:border-gray-700">
        <button
          onClick={handleScan}
          disabled={scanning}
          className="w-full flex items-center justify-center gap-2 px-4 py-2.5 bg-brand-600 text-white rounded-lg text-sm font-medium hover:bg-brand-700 transition-colors disabled:opacity-50"
        >
          <RefreshCw className={`w-4 h-4 ${scanning ? "animate-spin" : ""}`} />
          {scanning ? "Scanning..." : "Scan Now"}
        </button>
      </div>
    </aside>
  );
}
