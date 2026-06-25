import { useState } from "react";
import { Camera, ChevronDown, ChevronUp, RotateCcw, FileDiff, Tag, Plus, X as XIcon, GitCompareArrows, GitBranch } from "lucide-react";
import type { FileSnapshotRecord, SnapshotTag } from "../../lib/tauri";
import { formatBytes, timeAgo, restoreFileSnapshot, getSnapshotsForFile, getTagsForSnapshot, addSnapshotTag, removeSnapshotTag } from "../../lib/tauri";
import { FileDiffViewer } from "./FileDiffViewer";
import { SnapshotCompare } from "./SnapshotCompare";
import { BlameView } from "./BlameView";

interface SnapshotPanelProps {
  snapshotCount: number;
  totalSize: number;
}

export function SnapshotPanel({ snapshotCount, totalSize }: SnapshotPanelProps) {
  const [expanded, setExpanded] = useState(false);
  const [searchPath, setSearchPath] = useState("");
  const [fileSnapshots, setFileSnapshots] = useState<FileSnapshotRecord[]>([]);
  const [loading, setLoading] = useState(false);
  const [restoring, setRestoring] = useState<number | null>(null);
  const [diffSnapshot, setDiffSnapshot] = useState<FileSnapshotRecord | null>(null);
  const [tagsMap, setTagsMap] = useState<Record<number, SnapshotTag[]>>({});
  const [taggingSnapshot, setTaggingSnapshot] = useState<number | null>(null);
  const [newTagName, setNewTagName] = useState("");
  const [showCompare, setShowCompare] = useState(false);
  const [blameFilePath, setBlameFilePath] = useState<string | null>(null);

  const handleSearch = async () => {
    if (!searchPath.trim()) return;
    setLoading(true);
    try {
      const snapshots = await getSnapshotsForFile(searchPath);
      setFileSnapshots(snapshots);
      await loadTags(snapshots);
    } catch (err) {
      console.error("Failed to load snapshots:", err);
    } finally {
      setLoading(false);
    }
  };

  const loadTags = async (snapshots: FileSnapshotRecord[]) => {
    const tagResults = await Promise.all(
      snapshots.map(async (snap) => {
        try {
          const snapTags = await getTagsForSnapshot(snap.id);
          return { id: snap.id, tags: snapTags };
        } catch {
          return { id: snap.id, tags: [] as SnapshotTag[] };
        }
      })
    );
    const tags: Record<number, SnapshotTag[]> = {};
    for (const { id, tags: snapTags } of tagResults) {
      if (snapTags.length > 0) tags[id] = snapTags;
    }
    setTagsMap(tags);
  };

  const handleAddTag = async (snapshotId: number) => {
    if (!newTagName.trim()) return;
    try {
      await addSnapshotTag(snapshotId, newTagName.trim());
      setNewTagName("");
      setTaggingSnapshot(null);
      if (searchPath) {
        const snapshots = await getSnapshotsForFile(searchPath);
        setFileSnapshots(snapshots);
        await loadTags(snapshots);
      }
    } catch (err) {
      console.error("Failed to add tag:", err);
    }
  };

  const handleRemoveTag = async (tagId: number, _snapshotId: number) => {
    try {
      await removeSnapshotTag(tagId);
      if (searchPath) {
        const snapshots = await getSnapshotsForFile(searchPath);
        setFileSnapshots(snapshots);
        await loadTags(snapshots);
      }
    } catch (err) {
      console.error("Failed to remove tag:", err);
    }
  };

  const handleRestore = async (snapshot: FileSnapshotRecord) => {
    const confirmed = window.confirm(
      `Restore "${snapshot.original_filename}" to its original location?\n\n` +
      `Snapshot from: ${timeAgo(snapshot.created_at)}\n` +
      `Path: ${snapshot.original_path}\n` +
      `If a file exists there, it will be backed up first.`
    );
    if (!confirmed) return;

    setRestoring(snapshot.id);
    try {
      await restoreFileSnapshot(snapshot.id);
      if (searchPath) {
        const snapshots = await getSnapshotsForFile(searchPath);
        setFileSnapshots(snapshots);
      }
    } catch (err) {
      window.alert(`Snapshot restore failed: ${err}`);
    } finally {
      setRestoring(null);
    }
  };

  return (
    <div className="bg-white rounded-xl border border-gray-200 overflow-hidden">
      <div className="flex items-center justify-between p-5 border-b border-gray-100">
        <div className="flex items-center gap-3">
          <div className="w-10 h-10 rounded-full bg-blue-100 flex items-center justify-center">
            <Camera className="w-5 h-5 text-blue-600" />
          </div>
          <div>
            <h3 className="text-base font-bold text-gray-900">File Snapshots</h3>
            <p className="text-sm text-gray-500">
              {snapshotCount} versions ({formatBytes(totalSize)})
            </p>
          </div>
        </div>
        <div className="flex items-center gap-2">
          <button
            onClick={() => setShowCompare(true)}
            className="flex items-center gap-1 px-3 py-1.5 text-xs font-medium text-indigo-600 bg-indigo-50 rounded-lg hover:bg-indigo-100 transition-colors border border-indigo-200"
            title="Compare any two snapshots"
          >
            <GitCompareArrows className="w-3.5 h-3.5" />
            Compare
          </button>
          <button
            onClick={() => setExpanded(!expanded)}
            className="p-2 text-gray-400 hover:text-brand-500 hover:bg-gray-50 rounded-lg transition-colors"
          >
          {expanded ? <ChevronUp className="w-4 h-4" /> : <ChevronDown className="w-4 h-4" />}
        </button>
        </div>
      </div>

      {expanded && (
        <div className="p-5 space-y-4">
          <div className="flex gap-2">
            <input
              type="text"
              value={searchPath}
              onChange={(e) => setSearchPath(e.target.value)}
              placeholder="Enter file path to view versions..."
              className="flex-1 px-3 py-2 text-sm border border-gray-200 rounded-lg focus:outline-none focus:ring-2 focus:ring-brand-500 focus:border-transparent"
              onKeyDown={(e) => e.key === "Enter" && handleSearch()}
            />
            <button
              onClick={handleSearch}
              disabled={loading || !searchPath.trim()}
              className="px-4 py-2 text-sm font-medium text-white bg-brand-500 rounded-lg hover:bg-brand-600 disabled:opacity-50 transition-colors"
            >
              {loading ? "Loading..." : "Search"}
            </button>
          </div>

          {fileSnapshots.length > 0 && (
            <div className="space-y-2">
              {fileSnapshots.map((snap) => (
                <div key={snap.id} className="bg-gray-50 rounded-lg p-3">
                  <div className="flex items-center gap-3">
                    <Camera className="w-4 h-4 text-blue-400 flex-shrink-0" />
                    <div className="flex-1 min-w-0">
                      <p className="text-sm font-medium text-gray-900 truncate">
                        {snap.original_filename}
                      </p>
                      <p className="text-xs text-gray-400">
                        {formatBytes(snap.original_size)} → {formatBytes(snap.compressed_size)} compressed
                      </p>
                    </div>
                    <span className="text-xs text-gray-400 flex-shrink-0">
                      {timeAgo(snap.created_at)}
                    </span>
                    <button
                      onClick={() => setTaggingSnapshot(taggingSnapshot === snap.id ? null : snap.id)}
                      className="flex-shrink-0 p-1.5 text-gray-400 hover:text-brand-600 hover:bg-brand-50 rounded transition-colors"
                      title="Tag this snapshot"
                    >
                      <Tag className="w-4 h-4" />
                    </button>
                    <button
                      onClick={() => setDiffSnapshot(snap)}
                      className="flex-shrink-0 p-1.5 text-gray-400 hover:text-brand-600 hover:bg-brand-50 rounded transition-colors"
                      title="View line-by-line diff"
                    >
                      <FileDiff className="w-4 h-4" />
                    </button>
                    <button
                      onClick={() => setBlameFilePath(snap.original_path)}
                      className="flex-shrink-0 p-1.5 text-gray-400 hover:text-purple-600 hover:bg-purple-50 rounded transition-colors"
                      title="View blame (which scan introduced each line)"
                    >
                      <GitBranch className="w-4 h-4" />
                    </button>
                    <button
                      onClick={() => handleRestore(snap)}
                      disabled={restoring === snap.id}
                      className="flex-shrink-0 p-1.5 text-gray-400 hover:text-blue-600 hover:bg-blue-50 rounded transition-colors disabled:opacity-50"
                      title="Restore this version"
                    >
                      <RotateCcw className="w-4 h-4" />
                    </button>
                  </div>

                  {/* Tags */}
                  {tagsMap[snap.id] && tagsMap[snap.id].length > 0 && (
                    <div className="flex flex-wrap gap-1 mt-2 ml-7">
                      {tagsMap[snap.id].map((tag) => (
                        <span
                          key={tag.id}
                          className="inline-flex items-center gap-1 px-2 py-0.5 rounded-full text-[10px] font-medium text-white"
                          style={{ backgroundColor: tag.color || "#6366f1" }}
                        >
                          <Tag className="w-2.5 h-2.5" />
                          {tag.name}
                          <button
                            onClick={() => handleRemoveTag(tag.id, snap.id)}
                            className="hover:opacity-70"
                          >
                            <XIcon className="w-2.5 h-2.5" />
                          </button>
                        </span>
                      ))}
                    </div>
                  )}

                  {/* Tag input */}
                  {taggingSnapshot === snap.id && (
                    <div className="flex items-center gap-2 mt-2 ml-7">
                      <input
                        type="text"
                        value={newTagName}
                        onChange={(e) => setNewTagName(e.target.value)}
                        placeholder="Tag name (e.g., Pre-deploy, v1.0)"
                        className="flex-1 px-2 py-1 text-xs border border-gray-200 rounded focus:outline-none focus:ring-1 focus:ring-brand-500"
                        onKeyDown={(e) => e.key === "Enter" && handleAddTag(snap.id)}
                        autoFocus
                      />
                      <button
                        onClick={() => handleAddTag(snap.id)}
                        disabled={!newTagName.trim()}
                        className="px-2 py-1 bg-brand-600 text-white text-xs rounded hover:bg-brand-700 disabled:opacity-50"
                      >
                        <Plus className="w-3 h-3" />
                      </button>
                    </div>
                  )}
                </div>
              ))}
            </div>
          )}

          {searchPath && fileSnapshots.length === 0 && !loading && (
            <p className="text-sm text-gray-500 text-center py-4">
              No snapshots found for this file
            </p>
          )}

          {snapshotCount === 0 && (
            <div className="text-center py-6">
              <Camera className="w-8 h-8 mx-auto text-gray-300 mb-2" />
              <p className="text-sm text-gray-500">No file snapshots yet</p>
              <p className="text-xs text-gray-400 mt-1">
                Enable in Settings to start backing up text files
              </p>
            </div>
          )}
        </div>
      )}

      {/* Diff viewer modal */}
      {diffSnapshot && (
        <FileDiffViewer
          snapshotId={diffSnapshot.id}
          filePath={diffSnapshot.original_path}
          snapshotDate={diffSnapshot.created_at}
          onClose={() => setDiffSnapshot(null)}
        />
      )}

      {/* Snapshot Compare modal */}
      {showCompare && (
        <SnapshotCompare onClose={() => setShowCompare(false)} />
      )}

      {/* Blame View modal */}
      {blameFilePath && (
        <BlameView filePath={blameFilePath} onClose={() => setBlameFilePath(null)} />
      )}
    </div>
  );
}
