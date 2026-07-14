import { useState } from "react";
import { Search, Filter, X } from "lucide-react";
import { advancedSearch, parseDbTimestamp } from "../../lib/tauri";
import type { ChangeRecord, AdvancedSearchResult } from "../../lib/tauri";

interface AdvancedSearchBarProps {
  onSelectResult?: (record: ChangeRecord) => void;
}

export function AdvancedSearchBar({ onSelectResult }: AdvancedSearchBarProps) {
  const [query, setQuery] = useState("");
  const [showFilters, setShowFilters] = useState(false);
  const [loading, setLoading] = useState(false);
  const [results, setResults] = useState<AdvancedSearchResult | null>(null);
  const [expanded, setExpanded] = useState(false);

  const [filters, setFilters] = useState({
    change_type: "",
    date_from: "",
    date_to: "",
    extension: "",
    min_size: "",
    max_size: "",
  });

  const hasActiveFilters = Object.values(filters).some((v) => v !== "");

  const handleSearch = async () => {
    if (!query && !hasActiveFilters) return;
    setLoading(true);
    try {
      const res = await advancedSearch({
        query: query || undefined,
        change_type: filters.change_type || undefined,
        date_from: filters.date_from || undefined,
        date_to: filters.date_to || undefined,
        extension: filters.extension || undefined,
        min_size: filters.min_size ? Number(filters.min_size) : undefined,
        max_size: filters.max_size ? Number(filters.max_size) : undefined,
        limit: 50,
      });
      setResults(res);
      setExpanded(true);
    } catch (err) {
      console.error("Search failed:", err);
    } finally {
      setLoading(false);
    }
  };

  const handleClear = () => {
    setQuery("");
    setFilters({ change_type: "", date_from: "", date_to: "", extension: "", min_size: "", max_size: "" });
    setResults(null);
    setExpanded(false);
  };

  const handleKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === "Enter") handleSearch();
    if (e.key === "Escape") handleClear();
  };

  const changeTypeColors: Record<string, string> = {
    NEW: "bg-green-100 text-green-700",
    MODIFIED: "bg-blue-100 text-blue-700",
    DELETED: "bg-red-100 text-red-700",
    MOVED: "bg-yellow-100 text-yellow-700",
  };

  return (
    <div className="relative">
      {/* Search bar */}
      <div className="flex items-center gap-2">
        <div className="relative flex-1">
          <Search className="absolute left-3 top-1/2 -translate-y-1/2 w-4 h-4 text-gray-400" />
          <input
            type="text"
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            onKeyDown={handleKeyDown}
            placeholder="Search files, changes..."
            className="w-full pl-9 pr-3 py-2 bg-gray-50 border border-gray-200 rounded-lg text-sm focus:outline-none focus:ring-2 focus:ring-brand-500 focus:border-transparent"
          />
        </div>
        <button
          onClick={() => setShowFilters(!showFilters)}
          className={`p-2 rounded-lg border transition-colors ${
            showFilters || hasActiveFilters
              ? "bg-brand-50 border-brand-300 text-brand-600"
              : "bg-white border-gray-200 text-gray-500 hover:bg-gray-50"
          }`}
        >
          <Filter className="w-4 h-4" />
        </button>
        {(query || hasActiveFilters) && (
          <>
            <button
              onClick={handleSearch}
              disabled={loading}
              className="px-3 py-2 bg-brand-600 text-white rounded-lg text-sm font-medium hover:bg-brand-700 disabled:opacity-50"
            >
              {loading ? "..." : "Search"}
            </button>
            <button onClick={handleClear} className="p-2 text-gray-400 hover:text-gray-600">
              <X className="w-4 h-4" />
            </button>
          </>
        )}
      </div>

      {/* Filters panel */}
      {showFilters && (
        <div className="mt-2 p-3 bg-white border border-gray-200 rounded-lg shadow-sm">
          <div className="grid grid-cols-2 lg:grid-cols-4 gap-3">
            <div>
              <label className="block text-xs font-medium text-gray-500 mb-1">Change Type</label>
              <select
                value={filters.change_type}
                onChange={(e) => setFilters({ ...filters, change_type: e.target.value })}
                className="w-full px-2 py-1.5 bg-gray-50 border border-gray-200 rounded text-sm"
              >
                <option value="">All types</option>
                <option value="NEW">New</option>
                <option value="MODIFIED">Modified</option>
                <option value="DELETED">Deleted</option>
                <option value="MOVED">Moved</option>
              </select>
            </div>
            <div>
              <label className="block text-xs font-medium text-gray-500 mb-1">Extension</label>
              <input
                type="text"
                value={filters.extension}
                onChange={(e) => setFilters({ ...filters, extension: e.target.value })}
                placeholder=".txt, .rs..."
                className="w-full px-2 py-1.5 bg-gray-50 border border-gray-200 rounded text-sm"
              />
            </div>
            <div>
              <label className="block text-xs font-medium text-gray-500 mb-1">Date From</label>
              <input
                type="date"
                value={filters.date_from}
                onChange={(e) => setFilters({ ...filters, date_from: e.target.value })}
                className="w-full px-2 py-1.5 bg-gray-50 border border-gray-200 rounded text-sm"
              />
            </div>
            <div>
              <label className="block text-xs font-medium text-gray-500 mb-1">Date To</label>
              <input
                type="date"
                value={filters.date_to}
                onChange={(e) => setFilters({ ...filters, date_to: e.target.value })}
                className="w-full px-2 py-1.5 bg-gray-50 border border-gray-200 rounded text-sm"
              />
            </div>
          </div>
          <div className="grid grid-cols-2 gap-3 mt-2">
            <div>
              <label className="block text-xs font-medium text-gray-500 mb-1">Min Size (bytes)</label>
              <input
                type="number"
                value={filters.min_size}
                onChange={(e) => setFilters({ ...filters, min_size: e.target.value })}
                placeholder="0"
                className="w-full px-2 py-1.5 bg-gray-50 border border-gray-200 rounded text-sm"
              />
            </div>
            <div>
              <label className="block text-xs font-medium text-gray-500 mb-1">Max Size (bytes)</label>
              <input
                type="number"
                value={filters.max_size}
                onChange={(e) => setFilters({ ...filters, max_size: e.target.value })}
                placeholder="No limit"
                className="w-full px-2 py-1.5 bg-gray-50 border border-gray-200 rounded text-sm"
              />
            </div>
          </div>
        </div>
      )}

      {/* Results dropdown */}
      {expanded && results && (
        <div className="absolute top-full left-0 right-0 mt-2 bg-white border border-gray-200 rounded-lg shadow-lg z-50 max-h-96 overflow-y-auto">
          {results.records.length === 0 ? (
            <div className="p-4 text-center text-sm text-gray-400">No results found</div>
          ) : (
            <>
              <div className="px-3 py-2 border-b border-gray-100 text-xs text-gray-500">
                {results.total_count} results found
              </div>
              {results.records.map((record) => (
                <button
                  key={record.id}
                  onClick={() => onSelectResult?.(record)}
                  className="w-full px-3 py-2 flex items-center gap-3 hover:bg-gray-50 text-left border-b border-gray-50 last:border-0"
                >
                  <span className={`px-1.5 py-0.5 rounded text-xs font-medium ${changeTypeColors[record.change_type] || ""}`}>
                    {record.change_type}
                  </span>
                  <div className="flex-1 min-w-0">
                    <div className="text-sm font-medium text-gray-800 truncate">{record.filename}</div>
                    <div className="text-xs text-gray-400 truncate">{record.file_path}</div>
                  </div>
                  <span className="text-xs text-gray-400 whitespace-nowrap">
                    {parseDbTimestamp(record.detected_at).toLocaleDateString()}
                  </span>
                </button>
              ))}
            </>
          )}
          <button
            onClick={() => setExpanded(false)}
            className="w-full px-3 py-2 text-xs text-gray-500 hover:bg-gray-50 border-t border-gray-100"
          >
            Close results
          </button>
        </div>
      )}
    </div>
  );
}
