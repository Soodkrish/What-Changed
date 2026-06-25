import { ReactNode } from "react";
import { Sidebar } from "./Sidebar";

interface LayoutProps {
  children: ReactNode;
  currentView: string;
  onNavigate: (view: "dashboard" | "settings" | "duplicates" | "recovery" | "changelog") => void;
  dark: boolean;
  onToggleDark: () => void;
}

export function Layout({ children, currentView, onNavigate, dark, onToggleDark }: LayoutProps) {
  return (
    <div className="flex h-screen bg-gray-50 dark:bg-gray-950">
      <Sidebar currentView={currentView} onNavigate={onNavigate} dark={dark} onToggleDark={onToggleDark} />
      <main className="flex-1 overflow-auto">
        <div className="p-6">
          {children}
        </div>
      </main>
    </div>
  );
}
