import { useState, useEffect, useRef } from "react";
import { Search, X, FilePlus, FileEdit, FileX, ArrowRightLeft } from "lucide-react";
import type { ChangeRecord } from "../../lib/tauri";
import { searchChanges, parseDbTimestamp } from "../../lib/tauri";

const changeTypeConfig = {
  NEW: { icon: FilePlus, color: "text-emerald-600", bg: "bg-emerald-50" },
  MODIFIED: { icon: FileEdit, color: "text-blue-600", bg: "bg-blue-50" },
  DELETED: { icon: FileX, color: "text-red-600", bg: "bg-red-50" },
  MOVED: { icon: ArrowRightLeft, color: "text-amber-600", bg: "bg-amber-50" },
};

interface SearchBarProps {
  onResultClick?: (change: ChangeRecord) => void;
}

export function SearchBar({ onResultClick }: SearchBarProps) {
  const [query, setQuery] = useState("");
  const [results, setResults] = useState<ChangeRecord[]>([]);
  const [loading, setLoading] = useState(false);
  const [isOpen, setIsOpen] = useState(false);
  const inputRef = useRef<HTMLInputElement>(null);
  const debounceRef = useRef<ReturnType<typeof setTimeout>>();

  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      if ((e.ctrlKey || e.metaKey) && e.key === "k") {
        e.preventDefault();
        inputRef.current?.focus();
        setIsOpen(true);
      }
      if (e.key === "Escape") {
        setIsOpen(false);
        setQuery("");
      }
    };
    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, []);

  useEffect(() => {
    if (!query.trim()) {
      setResults([]);
      return;
    }

    if (debounceRef.current) clearTimeout(debounceRef.current);
    debounceRef.current = setTimeout(async () => {
      setLoading(true);
      try {
        const data = await searchChanges(query, 20);
        setResults(data);
      } catch (err) {
        console.error("Search failed:", err);
      } finally {
        setLoading(false);
      }
    }, 300);

    return () => {
      if (debounceRef.current) clearTimeout(debounceRef.current);
    };
  }, [query]);

  const formatDate = (dateStr: string) => {
    const date = parseDbTimestamp(dateStr);
    return date.toLocaleTimeString("en-US", {
      hour: "numeric",
      minute: "2-digit",
      hour12: true,
    });
  };

  return (
    <div className="relative">
      <div
        className="flex items-center gap-2 px-3 py-2 bg-gray-100 dark:bg-gray-800 rounded-lg cursor-pointer hover:bg-gray-200 dark:hover:bg-gray-700 transition-colors"
        onClick={() => {
          setIsOpen(true);
          inputRef.current?.focus();
        }}
      >
        <Search className="w-4 h-4 text-gray-400" />
        <input
          ref={inputRef}
          type="text"
          value={query}
          onChange={(e) => {
            setQuery(e.target.value);
            setIsOpen(true);
          }}
          onFocus={() => setIsOpen(true)}
          placeholder="Search changes... (Ctrl+K)"
          className="flex-1 bg-transparent text-sm text-gray-700 dark:text-gray-300 placeholder-gray-400 dark:placeholder-gray-500 outline-none"
        />
        {query && (
          <button
            onClick={(e) => {
              e.stopPropagation();
              setQuery("");
              setResults([]);
            }}
            className="p-1 text-gray-400 hover:text-gray-600"
          >
            <X className="w-3 h-3" />
          </button>
        )}
      </div>

      {isOpen && query.trim() && (
        <div className="absolute top-full left-0 right-0 mt-1 bg-white dark:bg-gray-900 border border-gray-200 dark:border-gray-700 rounded-lg shadow-lg z-50 max-h-80 overflow-auto">
          {loading ? (
            <div className="p-4 text-center text-sm text-gray-500 dark:text-gray-400">
              Searching...
            </div>
          ) : results.length === 0 ? (
            <div className="p-4 text-center text-sm text-gray-500 dark:text-gray-400">
              No results found
            </div>
          ) : (
            results.map((change) => {
              const config = changeTypeConfig[change.change_type as keyof typeof changeTypeConfig] || changeTypeConfig.MODIFIED;
              const Icon = config.icon;
              return (
                <div
                  key={change.id}
                  className="flex items-center gap-3 px-4 py-2.5 hover:bg-gray-50 dark:hover:bg-gray-800 cursor-pointer border-b border-gray-100 dark:border-gray-800 last:border-0"
                  onClick={() => {
                    onResultClick?.(change);
                    setIsOpen(false);
                    setQuery("");
                  }}
                >
                  <span className={`inline-flex items-center gap-1 px-2 py-0.5 rounded text-[10px] font-semibold ${config.bg} ${config.color}`}>
                    <Icon className="w-3 h-3" />
                    {change.change_type}
                  </span>
                  <span className="text-sm font-medium text-gray-900 dark:text-white truncate">
                    {change.filename}
                  </span>
                  <span className="text-xs text-gray-400 dark:text-gray-500">
                    {formatDate(change.detected_at)}
                  </span>
                </div>
              );
            })
          )}
        </div>
      )}
    </div>
  );
}
