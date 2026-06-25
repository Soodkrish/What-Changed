import { useState, useEffect } from "react";
import {
  Bell,
  BellOff,
  Plus,
  Trash2,
  Clock,
  FolderOpen,
  ChevronDown,
  ChevronUp,
  Check,
} from "lucide-react";
import type { NotificationProfile, MonitoredFolder } from "../../lib/tauri";
import {
  getAllNotificationProfiles,
  createNotificationProfile,
  deleteNotificationProfile,
  updateNotificationProfile,
  setNotificationProfileFolders,
  getFoldersForNotificationProfile,
  getMonitoredFolders,
} from "../../lib/tauri";

export function NotificationProfiles() {
  const [profiles, setProfiles] = useState<NotificationProfile[]>([]);
  const [loading, setLoading] = useState(true);
  const [expandedId, setExpandedId] = useState<number | null>(null);
  const [newName, setNewName] = useState("");
  const [showCreate, setShowCreate] = useState(false);
  const [allFolders, setAllFolders] = useState<MonitoredFolder[]>([]);
  const [profileFolders, setProfileFolders] = useState<Record<number, number[]>>({});

  useEffect(() => {
    loadProfiles();
    getMonitoredFolders().then(setAllFolders).catch(() => {});
  }, []);

  const loadProfiles = async () => {
    setLoading(true);
    try {
      const data = await getAllNotificationProfiles();
      setProfiles(data);
      // Load folder associations
      const folderMap: Record<number, number[]> = {};
      for (const p of data) {
        const folders = await getFoldersForNotificationProfile(p.id);
        folderMap[p.id] = folders.map((f) => f.id);
      }
      setProfileFolders(folderMap);
    } catch (err) {
      console.error("Failed to load profiles:", err);
    } finally {
      setLoading(false);
    }
  };

  const handleCreate = async () => {
    if (!newName.trim()) return;
    try {
      await createNotificationProfile(newName.trim());
      setNewName("");
      setShowCreate(false);
      await loadProfiles();
    } catch (err) {
      console.error("Failed to create profile:", err);
    }
  };

  const handleDelete = async (id: number) => {
    if (!window.confirm("Delete this notification profile?")) return;
    try {
      await deleteNotificationProfile(id);
      await loadProfiles();
    } catch (err) {
      console.error("Failed to delete profile:", err);
    }
  };

  const handleToggleType = async (id: number, field: string, current: boolean) => {
    try {
      await updateNotificationProfile(id, { [field]: !current });
      await loadProfiles();
    } catch (err) {
      console.error("Failed to update profile:", err);
    }
  };

  const handleToggleEnabled = async (id: number, current: boolean) => {
    try {
      await updateNotificationProfile(id, { enabled: !current });
      await loadProfiles();
    } catch (err) {
      console.error("Failed to toggle profile:", err);
    }
  };

  const handleQuietHoursChange = async (id: number, field: "quiet_hours_start" | "quiet_hours_end", value: string) => {
    const hour = parseInt(value, 10);
    if (isNaN(hour)) return;
    try {
      await updateNotificationProfile(id, { [field]: hour });
      await loadProfiles();
    } catch (err) {
      console.error("Failed to update quiet hours:", err);
    }
  };

  const handleToggleFolder = async (profileId: number, folderId: number) => {
    const current = profileFolders[profileId] || [];
    const next = current.includes(folderId)
      ? current.filter((f) => f !== folderId)
      : [...current, folderId];
    setProfileFolders((prev) => ({ ...prev, [profileId]: next }));
    try {
      await setNotificationProfileFolders(profileId, next);
    } catch (err) {
      console.error("Failed to update folders:", err);
      await loadProfiles();
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
          <Bell className="w-5 h-5 text-orange-500" />
          <h3 className="text-lg font-semibold text-gray-900">Notification Profiles</h3>
        </div>
        <button
          onClick={() => setShowCreate(!showCreate)}
          className="flex items-center gap-1 px-3 py-1.5 bg-brand-600 text-white rounded-lg text-sm font-medium hover:bg-brand-700 transition-colors"
        >
          <Plus className="w-3.5 h-3.5" />
          New
        </button>
      </div>
      <p className="text-sm text-gray-500 mb-4">
        Control which changes trigger notifications and when. Set quiet hours, filter by change type, and limit to specific folders.
      </p>

      {/* Create form */}
      {showCreate && (
        <div className="flex items-center gap-2 mb-4 p-3 bg-gray-50 rounded-lg">
          <input
            type="text"
            value={newName}
            onChange={(e) => setNewName(e.target.value)}
            placeholder="Profile name (e.g., Work Hours)"
            className="flex-1 px-3 py-1.5 text-sm border border-gray-200 rounded-lg focus:outline-none focus:ring-2 focus:ring-brand-500"
            onKeyDown={(e) => e.key === "Enter" && handleCreate()}
            autoFocus
          />
          <button
            onClick={handleCreate}
            disabled={!newName.trim()}
            className="px-3 py-1.5 bg-brand-600 text-white text-sm rounded-lg hover:bg-brand-700 disabled:opacity-50"
          >
            Create
          </button>
        </div>
      )}

      {/* Profiles list */}
      {profiles.length === 0 ? (
        <div className="text-center py-6 text-gray-400">
          <BellOff className="w-8 h-8 mx-auto mb-2" />
          <p className="text-sm">No notification profiles yet</p>
          <p className="text-xs mt-1">Create one to customize notification behavior</p>
        </div>
      ) : (
        <div className="space-y-3">
          {profiles.map((profile) => (
            <div key={profile.id} className="border border-gray-100 rounded-lg overflow-hidden">
              {/* Profile header */}
              <div className="flex items-center gap-3 px-4 py-3 bg-white">
                <button
                  onClick={() => handleToggleEnabled(profile.id, profile.enabled)}
                  className={`p-1 rounded ${profile.enabled ? "text-green-500" : "text-gray-300"}`}
                  title={profile.enabled ? "Disable" : "Enable"}
                >
                  {profile.enabled ? <Bell className="w-4 h-4" /> : <BellOff className="w-4 h-4" />}
                </button>
                <div className="flex-1 min-w-0">
                  <div className="flex items-center gap-2">
                    <span className="text-sm font-semibold text-gray-900">{profile.name}</span>
                    {!profile.enabled && (
                      <span className="text-[10px] font-medium text-gray-400 bg-gray-100 px-1.5 py-0.5 rounded">disabled</span>
                    )}
                  </div>
                  <div className="flex items-center gap-1 mt-1 flex-wrap">
                    {profile.notify_new && <span className="text-[10px] font-medium text-emerald-600 bg-emerald-50 px-1.5 py-0.5 rounded">+New</span>}
                    {profile.notify_modified && <span className="text-[10px] font-medium text-blue-600 bg-blue-50 px-1.5 py-0.5 rounded">~Modified</span>}
                    {profile.notify_deleted && <span className="text-[10px] font-medium text-red-600 bg-red-50 px-1.5 py-0.5 rounded">-Deleted</span>}
                    {profile.notify_moved && <span className="text-[10px] font-medium text-amber-600 bg-amber-50 px-1.5 py-0.5 rounded">→Moved</span>}
                    {profile.quiet_hours_start !== 0 || profile.quiet_hours_end !== 0 ? (
                      <span className="text-[10px] font-medium text-gray-500 bg-gray-100 px-1.5 py-0.5 rounded flex items-center gap-0.5">
                        <Clock className="w-2.5 h-2.5" />
                        {profile.quiet_hours_start}:00 – {profile.quiet_hours_end}:00
                      </span>
                    ) : null}
                  </div>
                </div>
                <button
                  onClick={() => setExpandedId(expandedId === profile.id ? null : profile.id)}
                  className="p-1 text-gray-400 hover:text-brand-500 rounded"
                >
                  {expandedId === profile.id ? <ChevronUp className="w-4 h-4" /> : <ChevronDown className="w-4 h-4" />}
                </button>
                <button
                  onClick={() => handleDelete(profile.id)}
                  className="p-1 text-gray-400 hover:text-red-500 rounded"
                >
                  <Trash2 className="w-4 h-4" />
                </button>
              </div>

              {/* Expanded settings */}
              {expandedId === profile.id && (
                <div className="px-4 pb-4 space-y-4 border-t border-gray-50">
                  {/* Change type toggles */}
                  <div className="pt-3">
                    <p className="text-xs font-medium text-gray-500 mb-2">Notify for:</p>
                    <div className="flex flex-wrap gap-2">
                      {[
                        { field: "notify_new", label: "New Files", color: "emerald" },
                        { field: "notify_modified", label: "Modified", color: "blue" },
                        { field: "notify_deleted", label: "Deleted", color: "red" },
                        { field: "notify_moved", label: "Moved", color: "amber" },
                      ].map(({ field, label, color }) => (
                        <button
                          key={field}
                          onClick={() => handleToggleType(profile.id, field, (profile as any)[field])}
                          className={`flex items-center gap-1 px-2.5 py-1 rounded-full text-xs font-medium border transition-colors ${
                            (profile as any)[field]
                              ? `bg-${color}-50 text-${color}-700 border-${color}-200`
                              : "bg-gray-50 text-gray-400 border-gray-200"
                          }`}
                        >
                          {(profile as any)[field] && <Check className="w-3 h-3" />}
                          {label}
                        </button>
                      ))}
                    </div>
                  </div>

                  {/* Quiet hours */}
                  <div>
                    <p className="text-xs font-medium text-gray-500 mb-2 flex items-center gap-1">
                      <Clock className="w-3 h-3" /> Quiet Hours (24h format)
                    </p>
                    <div className="flex items-center gap-2">
                      <select
                        value={profile.quiet_hours_start}
                        onChange={(e) => handleQuietHoursChange(profile.id, "quiet_hours_start", e.target.value)}
                        className="px-2 py-1 text-sm border border-gray-200 rounded focus:outline-none focus:ring-1 focus:ring-brand-500"
                      >
                        {Array.from({ length: 24 }, (_, i) => (
                          <option key={i} value={String(i)}>{String(i).padStart(2, "0")}:00</option>
                        ))}
                      </select>
                      <span className="text-gray-400 text-sm">to</span>
                      <select
                        value={profile.quiet_hours_end}
                        onChange={(e) => handleQuietHoursChange(profile.id, "quiet_hours_end", e.target.value)}
                        className="px-2 py-1 text-sm border border-gray-200 rounded focus:outline-none focus:ring-1 focus:ring-brand-500"
                      >
                        {Array.from({ length: 24 }, (_, i) => (
                          <option key={i} value={String(i)}>{String(i).padStart(2, "0")}:00</option>
                        ))}
                      </select>
                    </div>
                  </div>

                  {/* Folder filter */}
                  <div>
                    <p className="text-xs font-medium text-gray-500 mb-2 flex items-center gap-1">
                      <FolderOpen className="w-3 h-3" /> Limit to folders:
                    </p>
                    <p className="text-[10px] text-gray-400 mb-2">Leave empty to apply to all folders.</p>
                    <div className="flex flex-wrap gap-2">
                      {allFolders.map((folder) => {
                        const isActive = (profileFolders[profile.id] || []).includes(folder.id);
                        return (
                          <button
                            key={folder.id}
                            onClick={() => handleToggleFolder(profile.id, folder.id)}
                            className={`flex items-center gap-1 px-2.5 py-1 rounded-full text-xs font-medium border transition-colors ${
                              isActive
                                ? "bg-brand-50 text-brand-700 border-brand-200"
                                : "bg-gray-50 text-gray-400 border-gray-200"
                            }`}
                          >
                            {isActive && <Check className="w-3 h-3" />}
                            {folder.path.split(/[/\\]/).pop()}
                          </button>
                        );
                      })}
                    </div>
                  </div>
                </div>
              )}
            </div>
          ))}
        </div>
      )}
    </div>
  );
}
