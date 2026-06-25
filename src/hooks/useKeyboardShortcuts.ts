import { useEffect } from "react";

type View = "dashboard" | "settings" | "duplicates" | "recovery" | "changelog";

interface KeyboardShortcutOptions {
  onNavigate: (view: View) => void;
  onScan: () => void;
  onToggleDark: () => void;
  onToggleCompact?: () => void;
  onSearch?: () => void;
}

export function useKeyboardShortcuts({
  onNavigate,
  onScan,
  onToggleDark,
  onToggleCompact,
  onSearch,
}: KeyboardShortcutOptions) {
  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      // Don't trigger shortcuts when typing in inputs
      const target = e.target as HTMLElement;
      if (target.tagName === "INPUT" || target.tagName === "TEXTAREA" || target.tagName === "SELECT") {
        return;
      }

      // Ctrl/Cmd + key combos
      if (e.ctrlKey || e.metaKey) {
        switch (e.key.toLowerCase()) {
          case "k":
            e.preventDefault();
            onSearch?.();
            break;
          case "1":
            e.preventDefault();
            onNavigate("dashboard");
            break;
          case "2":
            e.preventDefault();
            onNavigate("duplicates");
            break;
          case "3":
            e.preventDefault();
            onNavigate("recovery");
            break;
          case "4":
            e.preventDefault();
            onNavigate("changelog");
            break;
          case "5":
            e.preventDefault();
            onNavigate("settings");
            break;
          case "d":
            e.preventDefault();
            onToggleDark();
            break;
          case "j":
            if (onToggleCompact) {
              e.preventDefault();
              onToggleCompact();
            }
            break;
        }
      }

      // Alt + key combos
      if (e.altKey) {
        switch (e.key.toLowerCase()) {
          case "s":
            e.preventDefault();
            onScan();
            break;
        }
      }
    };

    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, [onNavigate, onScan, onToggleDark, onToggleCompact, onSearch]);
}
