import { useState } from "react";
import { Keyboard, X } from "lucide-react";

const shortcuts = [
  { keys: "Ctrl+K", action: "Focus search" },
  { keys: "Ctrl+1-5", action: "Navigate views" },
  { keys: "Ctrl+D", action: "Toggle dark mode" },
  { keys: "Ctrl+J", action: "Toggle compact mode" },
  { keys: "Alt+S", action: "Run scan" },
  { keys: "Esc", action: "Close search / modal" },
];

export function ShortcutsHelp() {
  const [open, setOpen] = useState(false);

  return (
    <>
      <button
        onClick={() => setOpen(true)}
        className="p-2 text-gray-400 hover:text-brand-500 hover:bg-gray-100 dark:hover:bg-gray-800 rounded-lg transition-colors"
        title="Keyboard shortcuts"
      >
        <Keyboard className="w-4 h-4" />
      </button>

      {open && (
        <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/30">
          <div className="bg-white rounded-xl border border-gray-200 shadow-xl p-6 w-80">
            <div className="flex items-center justify-between mb-4">
              <h3 className="text-base font-bold text-gray-900 dark:text-white">Keyboard Shortcuts</h3>
              <button onClick={() => setOpen(false)} className="text-gray-400 hover:text-gray-600">
                <X className="w-4 h-4" />
              </button>
            </div>
            <div className="space-y-2">
              {shortcuts.map(({ keys, action }) => (
                <div key={keys} className="flex items-center justify-between py-1.5">
                  <span className="text-sm text-gray-600">{action}</span>
                  <kbd className="px-2 py-0.5 bg-gray-100 border border-gray-200 rounded text-xs font-mono text-gray-700">
                    {keys}
                  </kbd>
                </div>
              ))}
            </div>
          </div>
        </div>
      )}
    </>
  );
}
