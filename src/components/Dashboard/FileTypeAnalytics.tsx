import { useState, useEffect } from "react";
import { BarChart3, TrendingUp } from "lucide-react";
import { getExtensionStats, getDailyTrends } from "../../lib/tauri";
import type { ExtensionStat, DailyTrend } from "../../lib/tauri";
import { BarChart, Bar, XAxis, YAxis, Tooltip, ResponsiveContainer, PieChart, Pie, Cell, Legend } from "recharts";

const COLORS = ["#4f46e5", "#10b981", "#f59e0b", "#ef4444", "#8b5cf6", "#ec4899", "#06b6d4", "#84cc16", "#f97316", "#6366f1"];

export function FileTypeAnalytics() {
  const [extensions, setExtensions] = useState<ExtensionStat[]>([]);
  const [trends, setTrends] = useState<DailyTrend[]>([]);
  const [loading, setLoading] = useState(true);
  const [activeTab, setActiveTab] = useState<"types" | "trends">("types");

  useEffect(() => {
    Promise.all([getExtensionStats(), getDailyTrends(30)])
      .then(([exts, tr]) => { setExtensions(exts); setTrends(tr); })
      .catch(() => {})
      .finally(() => setLoading(false));
  }, []);

  const pieData = extensions.slice(0, 10).map((e) => ({
    name: e.extension || "(none)",
    value: e.count,
    size: e.total_size,
  }));

  const trendData = trends.map((t) => ({
    date: t.date.slice(5),
    New: t.new_count,
    Modified: t.modified_count,
    Deleted: t.deleted_count,
    Moved: t.moved_count,
  }));

  if (loading) {
    return (
      <div className="bg-white rounded-xl border border-gray-200 p-6">
        <div className="animate-pulse h-48 bg-gray-100 rounded" />
      </div>
    );
  }

  return (
    <div className="bg-white rounded-xl border border-gray-200 p-6">
      <div className="flex items-center justify-between mb-4">
        <div className="flex items-center gap-3">
          <div className="w-10 h-10 rounded-full bg-indigo-100 flex items-center justify-center">
            <BarChart3 className="w-5 h-5 text-indigo-600" />
          </div>
          <div>
            <h3 className="text-base font-bold text-gray-900 dark:text-white">File Analytics</h3>
            <p className="text-sm text-gray-500 dark:text-gray-400">Changes by file type & trends</p>
          </div>
        </div>
        <div className="flex gap-1 bg-gray-100 rounded-lg p-0.5">
          <button
            onClick={() => setActiveTab("types")}
            className={`px-3 py-1 text-xs font-medium rounded-md transition-colors ${
              activeTab === "types" ? "bg-white text-gray-900 shadow-sm" : "text-gray-500 hover:text-gray-700"
            }`}
          >
            By Type
          </button>
          <button
            onClick={() => setActiveTab("trends")}
            className={`px-3 py-1 text-xs font-medium rounded-md transition-colors ${
              activeTab === "trends" ? "bg-white text-gray-900 shadow-sm" : "text-gray-500 hover:text-gray-700"
            }`}
          >
            <TrendingUp className="w-3 h-3 inline mr-1" />
            Trends
          </button>
        </div>
      </div>

      {activeTab === "types" && (
        <div>
          {pieData.length === 0 ? (
            <p className="text-sm text-gray-400 text-center py-8">No file data yet. Scan a folder first.</p>
          ) : (
            <div className="flex flex-col lg:flex-row gap-6">
              <div className="flex-1">
                <ResponsiveContainer width="100%" height={250}>
                  <PieChart>
                    <Pie data={pieData} cx="50%" cy="50%" outerRadius={90} innerRadius={40} dataKey="value" paddingAngle={2}>
                      {pieData.map((_, i) => <Cell key={i} fill={COLORS[i % COLORS.length]} />)}
                    </Pie>
                    <Tooltip formatter={(v: number) => [`${v} files`, "Count"]} />
                    <Legend />
                  </PieChart>
                </ResponsiveContainer>
              </div>
              <div className="flex-1">
                <table className="w-full text-sm">
                  <thead><tr className="text-left text-xs text-gray-500"><th>Extension</th><th className="text-right">Count</th><th className="text-right">Size</th></tr></thead>
                  <tbody>
                    {extensions.slice(0, 10).map((ext) => (
                      <tr key={ext.extension} className="border-t border-gray-50">
                        <td className="py-1.5 font-medium text-gray-700"><code>{ext.extension}</code></td>
                        <td className="py-1.5 text-right text-gray-600">{ext.count}</td>
                        <td className="py-1.5 text-right text-gray-400">{(ext.total_size / 1024).toFixed(1)} KB</td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              </div>
            </div>
          )}
        </div>
      )}

      {activeTab === "trends" && (
        <div>
          {trendData.length === 0 ? (
            <p className="text-sm text-gray-400 text-center py-8">No trend data yet. Wait for some scans.</p>
          ) : (
            <ResponsiveContainer width="100%" height={280}>
              <BarChart data={trendData}>
                <XAxis dataKey="date" tick={{ fontSize: 11 }} />
                <YAxis tick={{ fontSize: 11 }} />
                <Tooltip />
                <Bar dataKey="New" fill="#10b981" stackId="a" />
                <Bar dataKey="Modified" fill="#3b82f6" stackId="a" />
                <Bar dataKey="Deleted" fill="#ef4444" stackId="a" />
                <Bar dataKey="Moved" fill="#f59e0b" stackId="a" />
              </BarChart>
            </ResponsiveContainer>
          )}
        </div>
      )}
    </div>
  );
}
