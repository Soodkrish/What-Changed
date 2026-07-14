import { useState, useEffect } from "react";
import {
  Webhook,
  Plus,
  Trash2,
  ToggleLeft,
  ToggleRight,
  Zap,
  ExternalLink,
  AlertCircle,
  CheckCircle,
  Bug,
} from "lucide-react";
import type { WebhookEndpoint } from "../../lib/tauri";
import {
  getAllWebhookEndpoints,
  createWebhookEndpoint,
  deleteWebhookEndpoint,
  toggleWebhookEndpoint,
  testWebhookEndpoint,
  diagnoseWebhooks,
  parseDbTimestamp,
} from "../../lib/tauri";

export function WebhookSettings() {
  const [endpoints, setEndpoints] = useState<WebhookEndpoint[]>([]);
  const [loading, setLoading] = useState(true);
  const [showCreate, setShowCreate] = useState(false);
  const [newName, setNewName] = useState("");
  const [newUrl, setNewUrl] = useState("");
  const [newEvents, setNewEvents] = useState("ALL");
  const [newSecret, setNewSecret] = useState("");
  const [testing, setTesting] = useState<number | null>(null);
  const [creating, setCreating] = useState(false);
  const [createError, setCreateError] = useState<string | null>(null);
  const [diagnostic, setDiagnostic] = useState<string | null>(null);
  const [diagLoading, setDiagLoading] = useState(false);

  useEffect(() => {
    loadEndpoints();
  }, []);

  const loadEndpoints = async () => {
    setLoading(true);
    try {
      setEndpoints(await getAllWebhookEndpoints());
    } catch (err) {
      console.error("Failed to load webhooks:", err);
    } finally {
      setLoading(false);
    }
  };

  const handleCreate = async () => {
    if (!newName.trim() || !newUrl.trim()) return;
    setCreating(true);
    setCreateError(null);
    try {
      await createWebhookEndpoint(
        newName.trim(),
        newUrl.trim(),
        newEvents.trim() || "ALL",
        newSecret.trim() || undefined,
      );
      setNewName("");
      setNewUrl("");
      setNewEvents("ALL");
      setNewSecret("");
      setShowCreate(false);
      setCreateError(null);
      await loadEndpoints();
    } catch (err: any) {
      const msg = typeof err === "string" ? err : err?.message || "Failed to create webhook";
      setCreateError(msg);
      console.error("Failed to create webhook:", err);
    } finally {
      setCreating(false);
    }
  };

  const handleDelete = async (id: number) => {
    if (!window.confirm("Delete this webhook endpoint?")) return;
    try {
      await deleteWebhookEndpoint(id);
      await loadEndpoints();
    } catch (err) {
      console.error("Failed to delete webhook:", err);
    }
  };

  const handleToggle = async (id: number, current: boolean) => {
    try {
      await toggleWebhookEndpoint(id, !current);
      await loadEndpoints();
    } catch (err) {
      console.error("Failed to toggle webhook:", err);
    }
  };

  const handleTest = async (id: number) => {
    setTesting(id);
    try {
      const status = await testWebhookEndpoint(id);
      if (status >= 200 && status < 300) {
        alert(`✅ Webhook test successful! Status: ${status}`);
      } else {
        alert(`⚠️ Webhook returned status: ${status}`);
      }
      await loadEndpoints();
    } catch (err) {
      alert(`❌ Webhook test failed: ${err}`);
    } finally {
      setTesting(null);
    }
  };

  const handleDiagnose = async () => {
    setDiagLoading(true);
    try {
      const report = await diagnoseWebhooks();
      setDiagnostic(report);
    } catch (err) {
      setDiagnostic(`Error running diagnostics: ${err}`);
    } finally {
      setDiagLoading(false);
    }
  };

  if (loading) {
    return (
      <div className="bg-white rounded-xl border border-gray-200 p-6">
        <div className="animate-pulse h-20 bg-gray-100 rounded" />
      </div>
    );
  }

  return (
    <div className="bg-white rounded-xl border border-gray-200 p-6">
      <div className="flex items-center justify-between mb-4">
        <div className="flex items-center gap-3">
          <Webhook className="w-5 h-5 text-violet-500" />
          <h3 className="text-lg font-semibold text-gray-900">Webhook Endpoints</h3>
        </div>
        <button
          onClick={() => setShowCreate(!showCreate)}
          className="flex items-center gap-1 px-3 py-1.5 bg-brand-600 text-white rounded-lg text-sm font-medium hover:bg-brand-700 transition-colors"
        >
          <Plus className="w-3.5 h-3.5" />
          Add Webhook
        </button>
      </div>
      <p className="text-sm text-gray-500 mb-4">
        POST change events to Slack, Discord, Microsoft Teams, or any custom URL.
        Supports HMAC signature verification via shared secret.
      </p>

      {/* Create form */}
      {showCreate && (
        <div className="mb-4 p-4 bg-gray-50 rounded-lg space-y-3">
          <div className="grid grid-cols-2 gap-3">
            <input
              type="text"
              value={newName}
              onChange={(e) => setNewName(e.target.value)}
              placeholder="Webhook name (e.g., Slack Alerts)"
              className="px-3 py-2 text-sm border border-gray-200 rounded-lg focus:outline-none focus:ring-2 focus:ring-brand-500 bg-white"
              autoFocus
            />
            <input
              type="text"
              value={newUrl}
              onChange={(e) => setNewUrl(e.target.value)}
              placeholder="https://hooks.slack.com/..."
              className="px-3 py-2 text-sm border border-gray-200 rounded-lg focus:outline-none focus:ring-2 focus:ring-brand-500 bg-white"
            />
          </div>
          <div className="grid grid-cols-2 gap-3">
            <div>
              <input
                type="text"
                value={newEvents}
                onChange={(e) => setNewEvents(e.target.value)}
                placeholder="Events: ALL, NEW, MODIFIED..."
                className="px-3 py-2 text-sm border border-gray-200 rounded-lg focus:outline-none focus:ring-2 focus:ring-brand-500 bg-white w-full"
              />
              <p className="text-[10px] text-gray-400 mt-1">Comma-separated: ALL, NEW, MODIFIED, DELETED, MOVED</p>
            </div>
            <div>
              <input
                type="password"
                value={newSecret}
                onChange={(e) => setNewSecret(e.target.value)}
                placeholder="Shared secret (optional)"
                className="px-3 py-2 text-sm border border-gray-200 rounded-lg focus:outline-none focus:ring-2 focus:ring-brand-500 bg-white w-full"
              />
              <p className="text-[10px] text-gray-400 mt-1">For HMAC signature in X-Webhook-Secret header</p>
            </div>
          </div>
          {createError && (
            <div className="px-3 py-2 bg-red-50 border border-red-200 rounded-lg text-sm text-red-700">
              {createError}
            </div>
          )}
          <div className="flex justify-end gap-2 items-center">
            <button
              onClick={() => { setShowCreate(false); setCreateError(null); }}
              className="px-4 py-1.5 text-gray-600 text-sm rounded-lg hover:bg-gray-100 transition-colors"
            >
              Cancel
            </button>
            <button
              onClick={handleCreate}
              disabled={!newName.trim() || !newUrl.trim() || creating}
              className="px-4 py-1.5 bg-brand-600 text-white text-sm rounded-lg hover:bg-brand-700 disabled:opacity-50 transition-colors"
            >
              {creating ? "Creating..." : "Create"}
            </button>
          </div>
        </div>
      )}

      {/* Endpoints list */}
      {endpoints.length === 0 ? (
        <div className="text-center py-6 text-gray-400">
          <Webhook className="w-8 h-8 mx-auto mb-2" />
          <p className="text-sm">No webhook endpoints configured</p>
          <p className="text-xs mt-1">Add one to forward change events to external services</p>
        </div>
      ) : (
        <div className="space-y-3">
          {endpoints.map((ep) => (
            <div key={ep.id} className="border border-gray-100 rounded-lg p-4">
              <div className="flex items-center gap-3">
                <button
                  onClick={() => handleToggle(ep.id, ep.enabled)}
                  className={`flex-shrink-0 ${ep.enabled ? "text-green-500" : "text-gray-300"}`}
                  title={ep.enabled ? "Disable" : "Enable"}
                >
                  {ep.enabled ? (
                    <ToggleRight className="w-6 h-6" />
                  ) : (
                    <ToggleLeft className="w-6 h-6" />
                  )}
                </button>
                <div className="flex-1 min-w-0">
                  <div className="flex items-center gap-2">
                    <span className="text-sm font-semibold text-gray-900">{ep.name}</span>
                    {!ep.enabled && (
                      <span className="text-[10px] font-medium text-gray-400 bg-gray-100 px-1.5 py-0.5 rounded">disabled</span>
                    )}
                  </div>
                  <div className="flex items-center gap-2 mt-1">
                    <ExternalLink className="w-3 h-3 text-gray-400 flex-shrink-0" />
                    <span className="text-xs text-gray-500 truncate">{ep.url}</span>
                  </div>
                  <div className="flex items-center gap-2 mt-1 flex-wrap">
                    <span className="text-[10px] font-medium text-violet-600 bg-violet-50 px-1.5 py-0.5 rounded">
                      {ep.events}
                    </span>
                    {ep.last_triggered && (
                      <span className="text-[10px] text-gray-400 flex items-center gap-0.5">
                        {ep.last_status && ep.last_status >= 200 && ep.last_status < 300 ? (
                          <CheckCircle className="w-3 h-3 text-green-400" />
                        ) : (
                          <AlertCircle className="w-3 h-3 text-red-400" />
                        )}
                        Last: {ep.last_status} ({parseDbTimestamp(ep.last_triggered).toLocaleDateString()})
                      </span>
                    )}
                    {ep.has_secret && (
                      <span className="text-[10px] font-medium text-gray-500 bg-gray-100 px-1.5 py-0.5 rounded">🔐 signed</span>
                    )}
                  </div>
                </div>
                <div className="flex items-center gap-1 flex-shrink-0">
                  <button
                    onClick={() => handleTest(ep.id)}
                    disabled={testing === ep.id}
                    className="p-1.5 text-gray-400 hover:text-brand-600 hover:bg-brand-50 rounded transition-colors disabled:opacity-50"
                    title="Send test ping"
                  >
                    <Zap className={`w-4 h-4 ${testing === ep.id ? "animate-pulse" : ""}`} />
                  </button>
                  <button
                    onClick={() => handleDelete(ep.id)}
                    className="p-1.5 text-gray-400 hover:text-red-500 hover:bg-red-50 rounded transition-colors"
                    title="Delete"
                  >
                    <Trash2 className="w-4 h-4" />
                  </button>
                </div>
              </div>
            </div>
          ))}
        </div>
      )}

      {/* Diagnose button */}
      <div className="mt-4 pt-4 border-t border-gray-100">
        <button
          onClick={handleDiagnose}
          disabled={diagLoading}
          className="flex items-center gap-1.5 px-3 py-1.5 text-xs font-medium text-gray-600 bg-gray-100 hover:bg-gray-200 rounded-lg transition-colors disabled:opacity-50"
        >
          <Bug className="w-3.5 h-3.5" />
          {diagLoading ? "Running..." : "Run Diagnostics"}
        </button>
      </div>

      {/* Diagnostic output */}
      {diagnostic && (
        <div className="mt-3 p-3 bg-gray-900 rounded-lg">
          <div className="flex items-center justify-between mb-2">
            <span className="text-xs font-semibold text-green-400">Diagnostic Report</span>
            <button onClick={() => setDiagnostic(null)} className="text-xs text-gray-500 hover:text-gray-300">✕</button>
          </div>
          <pre className="text-[11px] text-gray-300 whitespace-pre-wrap font-mono leading-relaxed overflow-auto max-h-80">
            {diagnostic}
          </pre>
        </div>
      )}
    </div>
  );
}
