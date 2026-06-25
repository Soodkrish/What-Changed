import { useState } from "react";
import { Download, FileJson, FileText, Globe, Loader2 } from "lucide-react";
import { exportDailyReport, generateHtmlReport, exportChangesCsv } from "../../lib/tauri";

interface ExportPanelProps {
  onExportComplete?: () => void;
}

export function ExportPanel({ onExportComplete }: ExportPanelProps) {
  const [selectedDate, setSelectedDate] = useState(() => {
    const now = new Date();
    return now.toISOString().split("T")[0];
  });
  const [format, setFormat] = useState<"json" | "csv" | "html">("json");
  const [exporting, setExporting] = useState(false);
  const [lastExport, setLastExport] = useState<string | null>(null);

  // Enhanced CSV with date range
  const [csvDateFrom, setCsvDateFrom] = useState("");
  const [csvDateTo, setCsvDateTo] = useState("");
  const [showCsvRange, setShowCsvRange] = useState(false);

  const handleExport = async () => {
    setExporting(true);
    setLastExport(null);
    try {
      let content: string;
      if (format === "html") {
        content = await generateHtmlReport();
      } else {
        content = await exportDailyReport(selectedDate, format);
      }
      setLastExport(content);
      onExportComplete?.();
    } catch (err) {
      console.error("Export failed:", err);
    } finally {
      setExporting(false);
    }
  };

  const handleCsvDateRange = async () => {
    setExporting(true);
    setLastExport(null);
    try {
      const content = await exportChangesCsv(csvDateFrom || undefined, csvDateTo || undefined);
      setLastExport(content);
      onExportComplete?.();
    } catch (err) {
      console.error("CSV export failed:", err);
    } finally {
      setExporting(false);
    }
  };

  const handleDownload = () => {
    if (!lastExport) return;
    const mimeTypes: Record<string, string> = {
      json: "application/json",
      csv: "text/csv",
      html: "text/html",
    };
    const extensions: Record<string, string> = {
      json: "json",
      csv: "csv",
      html: "html",
    };
    const blob = new Blob([lastExport], { type: mimeTypes[format] });
    const url = URL.createObjectURL(blob);
    const a = document.createElement("a");
    a.href = url;
    a.download = `whatchanged-report-${selectedDate}.${extensions[format]}`;
    document.body.appendChild(a);
    a.click();
    document.body.removeChild(a);
    URL.revokeObjectURL(url);
  };

  return (
    <div className="bg-white rounded-xl border border-gray-200 overflow-hidden">
      <div className="flex items-center gap-3 p-5 border-b border-gray-100">
        <div className="w-10 h-10 rounded-full bg-violet-100 flex items-center justify-center">
          <Download className="w-5 h-5 text-violet-600" />
        </div>
        <div>
          <h3 className="text-base font-bold text-gray-900">Export Report</h3>
          <p className="text-sm text-gray-500">
            Download change data as JSON, CSV, or HTML report
          </p>
        </div>
      </div>

      <div className="p-5 space-y-4">
        {/* Format selector */}
        <div>
          <label className="block text-sm font-medium text-gray-700 mb-2">Format</label>
          <div className="flex gap-2">
            {[
              { key: "json" as const, icon: FileJson, label: "JSON" },
              { key: "csv" as const, icon: FileText, label: "CSV" },
              { key: "html" as const, icon: Globe, label: "HTML Report" },
            ].map(({ key, icon: Icon, label }) => (
              <button
                key={key}
                onClick={() => setFormat(key)}
                className={`flex-1 flex items-center justify-center gap-2 px-3 py-2.5 text-sm font-medium rounded-lg border transition-colors ${
                  format === key
                    ? "bg-brand-50 text-brand-700 border-brand-300"
                    : "bg-white text-gray-600 border-gray-200 hover:bg-gray-50"
                }`}
              >
                <Icon className="w-4 h-4" />
                {label}
              </button>
            ))}
          </div>
        </div>

        {/* Date picker for JSON/HTML */}
        {format !== "csv" && (
          <div>
            <label className="block text-sm font-medium text-gray-700 mb-1">Date</label>
            <input
              type="date"
              value={selectedDate}
              onChange={(e) => setSelectedDate(e.target.value)}
              className="w-full px-3 py-2 text-sm border border-gray-200 rounded-lg focus:outline-none focus:ring-2 focus:ring-brand-500 focus:border-transparent"
            />
          </div>
        )}

        {/* CSV date range toggle */}
        {format === "csv" && (
          <div className="space-y-3">
            <button
              onClick={() => setShowCsvRange(!showCsvRange)}
              className="text-xs text-brand-600 hover:text-brand-700 font-medium"
            >
              {showCsvRange ? "Use single date" : "Use date range instead"}
            </button>
            {showCsvRange ? (
              <div className="grid grid-cols-2 gap-3">
                <div>
                  <label className="block text-xs font-medium text-gray-500 mb-1">From</label>
                  <input
                    type="date"
                    value={csvDateFrom}
                    onChange={(e) => setCsvDateFrom(e.target.value)}
                    className="w-full px-3 py-2 text-sm border border-gray-200 rounded-lg focus:outline-none focus:ring-2 focus:ring-brand-500"
                  />
                </div>
                <div>
                  <label className="block text-xs font-medium text-gray-500 mb-1">To</label>
                  <input
                    type="date"
                    value={csvDateTo}
                    onChange={(e) => setCsvDateTo(e.target.value)}
                    className="w-full px-3 py-2 text-sm border border-gray-200 rounded-lg focus:outline-none focus:ring-2 focus:ring-brand-500"
                  />
                </div>
              </div>
            ) : (
              <div>
                <label className="block text-sm font-medium text-gray-700 mb-1">Date</label>
                <input
                  type="date"
                  value={selectedDate}
                  onChange={(e) => setSelectedDate(e.target.value)}
                  className="w-full px-3 py-2 text-sm border border-gray-200 rounded-lg focus:outline-none focus:ring-2 focus:ring-brand-500"
                />
              </div>
            )}
          </div>
        )}

        {/* Generate button */}
        <div className="flex gap-2">
          {format === "csv" && showCsvRange ? (
            <button
              onClick={handleCsvDateRange}
              disabled={exporting}
              className="flex-1 flex items-center justify-center gap-2 px-4 py-2.5 text-sm font-medium text-white bg-brand-500 rounded-lg hover:bg-brand-600 disabled:opacity-50 transition-colors"
            >
              {exporting ? <Loader2 className="w-4 h-4 animate-spin" /> : <Download className="w-4 h-4" />}
              {exporting ? "Generating..." : "Export CSV Range"}
            </button>
          ) : (
            <button
              onClick={handleExport}
              disabled={exporting}
              className="flex-1 flex items-center justify-center gap-2 px-4 py-2.5 text-sm font-medium text-white bg-brand-500 rounded-lg hover:bg-brand-600 disabled:opacity-50 transition-colors"
            >
              {exporting ? <Loader2 className="w-4 h-4 animate-spin" /> : <Download className="w-4 h-4" />}
              {exporting ? "Generating..." : `Generate ${format.toUpperCase()}`}
            </button>
          )}
          {lastExport && (
            <button
              onClick={handleDownload}
              className="px-4 py-2.5 text-sm font-medium text-brand-600 bg-brand-50 border border-brand-200 rounded-lg hover:bg-brand-100 transition-colors"
            >
              <Download className="w-4 h-4 inline mr-1" />
              Download
            </button>
          )}
        </div>

        {lastExport && (
          <div className="p-3 bg-gray-50 rounded-lg">
            <p className="text-xs text-gray-500">
              Report generated: {(lastExport.length / 1024).toFixed(1)} KB
              {" "}{format.toUpperCase()}
            </p>
          </div>
        )}
      </div>
    </div>
  );
}
