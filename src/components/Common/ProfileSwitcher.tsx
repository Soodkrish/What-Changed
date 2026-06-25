import { useState, useEffect } from "react";
import { Layers, Plus, Check, Trash2, Save } from "lucide-react";
import {
  getAllProfiles,
  createProfile,
  deleteProfile,
  activateProfile,
  saveCurrentFoldersToProfile,
  type WorkspaceProfile,
} from "../../lib/tauri";

interface ProfileSwitcherProps {
  onProfileChanged?: () => void;
}

export function ProfileSwitcher({ onProfileChanged }: ProfileSwitcherProps) {
  const [profiles, setProfiles] = useState<WorkspaceProfile[]>([]);
  const [expanded, setExpanded] = useState(false);
  const [newName, setNewName] = useState("");
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    loadProfiles();
  }, []);

  const loadProfiles = async () => {
    setLoading(true);
    try {
      const data = await getAllProfiles();
      setProfiles(data);
    } catch (err) {
      console.error("Failed to load profiles:", err);
    } finally {
      setLoading(false);
    }
  };

  const activeProfile = profiles.find((p) => p.is_active);

  const handleCreate = async () => {
    if (!newName.trim()) return;
    try {
      const id = await createProfile(newName.trim());
      // Save current folders to the new profile
      await saveCurrentFoldersToProfile(id);
      setNewName("");
      await loadProfiles();
    } catch (err) {
      console.error("Failed to create profile:", err);
    }
  };

  const handleActivate = async (id: number) => {
    try {
      await activateProfile(id);
      await loadProfiles();
      onProfileChanged?.();
    } catch (err) {
      console.error("Failed to activate profile:", err);
    }
  };

  const handleDelete = async (id: number) => {
    try {
      await deleteProfile(id);
      await loadProfiles();
    } catch (err) {
      console.error("Failed to delete profile:", err);
    }
  };

  const handleSaveToActive = async () => {
    if (!activeProfile) return;
    try {
      await saveCurrentFoldersToProfile(activeProfile.id);
      await loadProfiles();
    } catch (err) {
      console.error("Failed to save profile:", err);
    }
  };

  return (
    <div className="border-t border-gray-200 dark:border-gray-700 p-4">
      <button
        onClick={() => setExpanded(!expanded)}
        className="w-full flex items-center gap-2 px-3 py-2 text-left rounded-lg hover:bg-gray-100 dark:hover:bg-gray-800 transition-colors"
      >
        <Layers className="w-4 h-4 text-gray-500 dark:text-gray-400" />
        <div className="flex-1 min-w-0">
          <p className="text-xs font-medium text-gray-600 dark:text-gray-400">Workspace</p>
          <p className="text-xs text-gray-500 dark:text-gray-500 truncate">
            {activeProfile ? activeProfile.name : "No active profile"}
          </p>
        </div>
        {expanded ? (
          <svg className="w-4 h-4 text-gray-400" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M19 9l-7 7-7-7" />
          </svg>
        ) : (
          <svg className="w-4 h-4 text-gray-400" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M9 5l7 7-7 7" />
          </svg>
        )}
      </button>

      {expanded && (
        <div className="mt-2 space-y-1">
          {loading ? (
            <p className="text-xs text-gray-400 dark:text-gray-500 px-3 py-1">Loading...</p>
          ) : profiles.length === 0 ? (
            <p className="text-xs text-gray-400 dark:text-gray-500 px-3 py-1">No profiles yet</p>
          ) : (
            profiles.map((p) => (
              <div
                key={p.id}
                className={`flex items-center gap-2 px-3 py-2 rounded-lg transition-colors ${
                  p.is_active
                    ? "bg-brand-50 dark:bg-brand-900/20"
                    : "hover:bg-gray-50 dark:hover:bg-gray-800"
                }`}
              >
                <button
                  onClick={() => handleActivate(p.id)}
                  className="flex-1 flex items-center gap-2 text-left min-w-0"
                >
                  {p.is_active && <Check className="w-3 h-3 text-brand-600 dark:text-brand-400 flex-shrink-0" />}
                  <span className={`text-xs truncate ${p.is_active ? "font-medium text-brand-700 dark:text-brand-300" : "text-gray-600 dark:text-gray-400"}`}>
                    {p.name}
                  </span>
                </button>
                {p.is_active && (
                  <button
                    onClick={handleSaveToActive}
                    className="p-1 text-gray-400 hover:text-brand-500 transition-colors"
                    title="Save current folders to this profile"
                  >
                    <Save className="w-3 h-3" />
                  </button>
                )}
                <button
                  onClick={() => handleDelete(p.id)}
                  className="p-1 text-gray-400 hover:text-red-500 transition-colors"
                  title="Delete profile"
                >
                  <Trash2 className="w-3 h-3" />
                </button>
              </div>
            ))
          )}

          {/* Create new profile */}
          <div className="flex gap-1 mt-2">
            <input
              type="text"
              value={newName}
              onChange={(e) => setNewName(e.target.value)}
              placeholder="New profile name..."
              className="flex-1 px-2 py-1 text-xs border border-gray-200 rounded dark:bg-gray-800 dark:border-gray-600 dark:text-white dark:placeholder-gray-500"
              onKeyDown={(e) => e.key === "Enter" && handleCreate()}
            />
            <button
              onClick={handleCreate}
              disabled={!newName.trim()}
              className="p-1 px-2 bg-brand-600 text-white rounded text-xs hover:bg-brand-700 disabled:opacity-50"
            >
              <Plus className="w-3 h-3" />
            </button>
          </div>
        </div>
      )}
    </div>
  );
}
