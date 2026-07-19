import { useState, useEffect, useCallback } from "react";
import {
  Camera, ChevronDown, ChevronUp, RotateCcw, Download, FileDiff, Tag, Plus,
  X as XIcon, GitCompareArrows, GitBranch, Search, FileCheck, FileX, Clock,
} from "lucide-react";
import type { FileSnapshotRecord, SnapshotFileGroup, SnapshotTag } from "../../lib/tauri";
import {
  formatBytes, timeAgo, restoreFileSnapshot, saveSnapshotToFile,
  getSnapshotsGroupedByFile, getSnapshotsForFile,
  getTagsForSnapshot, addSnapshotTag, removeSnapshotTag,
} from "../../lib/tauri";
import { save } from "@tauri-apps/plugin-dialog";
import { FileDiffViewer } from "./FileDiffViewer";
import { SnapshotCompare } from "./SnapshotCompare";
import { BlameView } from "./BlameView";

interface SnapshotPanelProps {
  snapshotCount: number;
  totalSize: number;
}

export function SnapshotPanel({ snapshotCount, totalSize }: SnapshotPanelProps) {
  const [expanded, setExpanded] = useState(false);
  const [files, setFiles] = useState<SnapshotFileGroup[]>([]);
  const [loading, setLoading] = useState(false);
  const [filter, setFilter] = useState("");

  // Expanded file → its snapshots
  const [expandedFile, setExpandedFile] = useState<string | null>(null);
  const [fileSnapshots, setFileSnapshots] = useState<FileSnapshotRecord[]>([]);
  const [snapshotsLoading, setSnapshotsLoading] = useState(false);

  const [restoring, setRestoring] = useState<number | null>(null);
  const [downloading, setDownloading] = useState<number | null>(null);
  const [diffSnapshot, setDiffSnapshot] = useState<FileSnapshotRecord | null>(null);
  const [tagsMap, setTagsMap] = useState<Record<number, SnapshotTag[]>>({});
  const [taggingSnapshot, setTaggingSnapshot] = useState<number | null>(null);
  const [newTagName, setNewTagName] = useState("");
  const [showCompare, setShowCompare] = useState(false);
  const [blameFilePath, setBlameFilePath] = useState<string | null>(null);

  const loadFiles = useCallback(async () => {
    setLoading(true);
    try {
      setFiles(await getSnapshotsGroupedByFile());
    } catch (err) {
      console.error("Failed to load snapshot files:", err);
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    if (expanded) loadFiles();
  }, [expanded, loadFiles]);

  const loadTags = async (snapshots: FileSnapshotRecord[]) => {
    const results = await Promise.all(
      snapshots.map(async (snap) => {
        try {
          const snapTags = await getTagsForSnapshot(snap.id);
          return { id: snap.id, tags: snapTags };
        } catch {
          return { id: snap.id, tags: [] as SnapshotTag[] };
        }
      })
    );
    const map: Record<number, SnapshotTag[]> = {};
    for (const { id, tags: snapTags } of results) {
      if (snapTags.length > 0) map[id] = snapTags;
    }
    setTagsMap(map);
  };

  const handleExpandFile = async (filePath: string) => {
    if (expandedFile === filePath) {
      setExpandedFile(null);
      setFileSnapshots([]);
      return;
    }
    setExpandedFile(filePath);
    setSnapshotsLoading(true);
    try {
      const snaps = await getSnapshotsForFile(filePath);
      setFileSnapshots(snaps);
      await loadTags(snaps);
    } catch (err) {
      console.error("Failed to load snapshots:", err);
    } finally {
      setSnapshotsLoading(false);
    }
  };

  const refreshCurrentFile = async () => {
    if (expandedFile) {
      const snaps = await getSnapshotsForFile(expandedFile);
      setFileSnapshots(snaps);
      await loadTags(snaps);
    }
    await loadFiles();
  };

  const handleAddTag = async (snapshotId: number) => {
    if (!newTagName.trim()) return;
    try {
      await addSnapshotTag(snapshotId, newTagName.trim());
      setNewTagName("");
      setTaggingSnapshot(null);
      await refreshCurrentFile();
    } catch (err) {
      console.error("Failed to add tag:", err);
    }
  };

  const handleRemoveTag = async (tagId: number) => {
    try {
      await removeSnapshotTag(tagId);
      await refreshCurrentFile();
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
      await refreshCurrentFile();
    } catch (err) {
      window.alert(`Restore failed: ${err}`);
    } finally {
      setRestoring(null);
    }
  };

  const handleDownload = async (snapshot: FileSnapshotRecord) => {
    try {
      const filePath = await save({
        defaultPath: snapshot.original_filename || "snapshot.txt",
        filters: [{ name: "All Files", extensions: ["*"] }],
      });
      if (!filePath) return;
      setDownloading(snapshot.id);
      await saveSnapshotToFile(snapshot.id, filePath);
      window.alert(`✅ Saved to:\n${filePath}`);
    } catch (err) {
      window.alert(`Download failed: ${err}`);
    } finally {
      setDownloading(null);
    }
  };

  const filteredFiles = files.filter((f) =>
    !filter || f.original_path.toLowerCase().includes(filter.toLowerCase())
  );

  return (
    <div className="bg-white dark:bg-gray-800 rounded-xl border border-gray-200 dark:border-gray-700 overflow-hidden">
      {/* Header */}
      <div className="flex items-center justify-between p-5 border-b border-gray-100">
        <div className="flex items-center gap-3">
          <div className="w-10 h-10 rounded-full bg-blue-100 flex items-center justify-center">
            <Camera className="w-5 h-5 text-blue-600" />
          </div>
          <div>
            <h3 className="text-base font-bold text-gray-900 dark:text-white">File Snapshots</h3>
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
          {/* Filter */}
          {snapshotCount > 0 && (
            <div className="relative">
              <Search className="absolute left-3 top-1/2 -translate-y-1/2 w-4 h-4 text-gray-400" />
              <input
                type="text"
                value={filter}
                onChange={(e) => setFilter(e.target.value)}
                placeholder="Filter files by name or path..."
                className="w-full pl-9 pr-3 py-2 text-sm border border-gray-200 rounded-lg focus:outline-none focus:ring-2 focus:ring-brand-500 focus:border-transparent"
              />
            </div>
          )}

          {/* Loading */}
          {loading && (
            <div className="text-center py-6">
              <div className="animate-spin w-6 h-6 border-2 border-brand-500 border-t-transparent rounded-full mx-auto" />
              <p className="text-sm text-gray-400 mt-2">Loading snapshots...</p>
            </div>
          )}

          {/* File list */}
          {!loading && filteredFiles.length > 0 && (
            <div className="space-y-2">
              {filteredFiles.map((file) => (
                <div key={file.original_path} className="border border-gray-100 rounded-lg overflow-hidden">
                  {/* File row */}
                  <button
                    onClick={() => handleExpandFile(file.original_path)}
                    className="w-full flex items-center gap-3 p-3 hover:bg-gray-50 transition-colors text-left"
                  >
                    {file.file_exists ? (
                      <FileCheck className="w-4 h-4 text-green-500 flex-shrink-0" />
                    ) : (
                      <FileX className="w-4 h-4 text-red-400 flex-shrink-0" />
                    )}
                    <div className="flex-1 min-w-0">
                      <p className="text-sm font-medium text-gray-900 truncate">{file.original_filename}</p>
                      <p className="text-[11px] text-gray-400 truncate">{file.original_path}</p>
                    </div>
                    <div className="flex items-center gap-3 flex-shrink-0">
                      <span className="text-[10px] font-medium text-blue-600 bg-blue-50 px-2 py-0.5 rounded-full">
                        {file.snapshot_count} {file.snapshot_count === 1 ? "version" : "versions"}
                      </span>
                      <span className="text-[10px] text-gray-400">{formatBytes(file.total_size)}</span>
                      {!file.file_exists && (
                        <span className="text-[10px] font-medium text-red-500 bg-red-50 px-2 py-0.5 rounded-full">
                          deleted
                        </span>
                      )}
                      {expandedFile === file.original_path ? (
                        <ChevronUp className="w-4 h-4 text-gray-400" />
                      ) : (
                        <ChevronDown className="w-4 h-4 text-gray-400" />
                      )}
                    </div>
                  </button>

                  {/* Expanded snapshots */}
                  {expandedFile === file.original_path && (
                    <div className="border-t border-gray-100 bg-gray-50/50 p-3 space-y-2">
                      {snapshotsLoading ? (
                        <div className="text-center py-3">
                          <div className="animate-spin w-4 h-4 border-2 border-brand-500 border-t-transparent rounded-full mx-auto" />
                        </div>
                      ) : fileSnapshots.length === 0 ? (
                        <p className="text-xs text-gray-400 text-center py-2">No snapshots found</p>
                      ) : (
                        fileSnapshots.map((snap, idx) => (
                          <div key={snap.id} className="bg-white rounded-lg p-3 border border-gray-100">
                            <div className="flex items-center gap-2">
                              <Clock className="w-3.5 h-3.5 text-gray-400 flex-shrink-0" />
                              <div className="flex-1 min-w-0">
                                <div className="flex items-center gap-2">
                                  <span className="text-xs font-medium text-gray-700">
                                    {new Date(snap.created_at).toLocaleString()}
                                  </span>
                                  {idx === 0 && (
                                    <span className="text-[9px] font-bold text-green-600 bg-green-50 px-1.5 py-0.5 rounded">
                                      LATEST
                                    </span>
                                  )}
                                </div>
                                <p className="text-[11px] text-gray-400">
                                  {formatBytes(snap.original_size)} → {formatBytes(snap.compressed_size)} compressed
                                </p>
                              </div>

                              {/* Action buttons */}
                              <div className="flex items-center gap-0.5 flex-shrink-0">
                                <button
                                  onClick={() => setTaggingSnapshot(taggingSnapshot === snap.id ? null : snap.id)}
                                  className="p-1.5 text-gray-400 hover:text-brand-600 hover:bg-brand-50 rounded transition-colors"
                                  title="Tag"
                                >
                                  <Tag className="w-3.5 h-3.5" />
                                </button>
                                <button
                                  onClick={() => setDiffSnapshot(snap)}
                                  className="p-1.5 text-gray-400 hover:text-brand-600 hover:bg-brand-50 rounded transition-colors"
                                  title="Diff"
                                >
                                  <FileDiff className="w-3.5 h-3.5" />
                                </button>
                                <button
                                  onClick={() => setBlameFilePath(snap.original_path)}
                                  className="p-1.5 text-gray-400 hover:text-purple-600 hover:bg-purple-50 rounded transition-colors"
                                  title="Blame"
                                >
                                  <GitBranch className="w-3.5 h-3.5" />
                                </button>
                                <button
                                  onClick={() => handleDownload(snap)}
                                  disabled={downloading === snap.id}
                                  className="p-1.5 text-gray-400 hover:text-green-600 hover:bg-green-50 rounded transition-colors disabled:opacity-50"
                                  title="Download"
                                >
                                  <Download className={`w-3.5 h-3.5 ${downloading === snap.id ? "animate-pulse" : ""}`} />
                                </button>
                                <button
                                  onClick={() => handleRestore(snap)}
                                  disabled={restoring === snap.id}
                                  className="p-1.5 text-gray-400 hover:text-blue-600 hover:bg-blue-50 rounded transition-colors disabled:opacity-50"
                                  title="Restore to original location"
                                >
                                  <RotateCcw className="w-3.5 h-3.5" />
                                </button>
                              </div>
                            </div>

                            {/* Tags */}
                            {tagsMap[snap.id] && tagsMap[snap.id].length > 0 && (
                              <div className="flex flex-wrap gap-1 mt-2 ml-6">
                                {tagsMap[snap.id].map((tag) => (
                                  <span
                                    key={tag.id}
                                    className="inline-flex items-center gap-1 px-2 py-0.5 rounded-full text-[10px] font-medium text-white"
                                    style={{ backgroundColor: tag.color || "#6366f1" }}
                                  >
                                    <Tag className="w-2.5 h-2.5" />
                                    {tag.name}
                                    <button onClick={() => handleRemoveTag(tag.id)} className="hover:opacity-70">
                                      <XIcon className="w-2.5 h-2.5" />
                                    </button>
                                  </span>
                                ))}
                              </div>
                            )}

                            {/* Tag input */}
                            {taggingSnapshot === snap.id && (
                              <div className="flex items-center gap-2 mt-2 ml-6">
                                <input
                                  type="text"
                                  value={newTagName}
                                  onChange={(e) => setNewTagName(e.target.value)}
                                  placeholder="Tag name (e.g., Pre-deploy)"
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
                        ))
                      )}
                    </div>
                  )}
                </div>
              ))}
            </div>
          )}

          {/* Empty states */}
          {!loading && snapshotCount === 0 && (
            <div className="text-center py-6">
              <Camera className="w-8 h-8 mx-auto text-gray-300 mb-2" />
              <p className="text-sm text-gray-500">No file snapshots yet</p>
              <p className="text-xs text-gray-400 mt-1">
                Enable in Settings to start backing up text files
              </p>
            </div>
          )}

          {!loading && snapshotCount > 0 && filteredFiles.length === 0 && (
            <p className="text-sm text-gray-500 text-center py-4">
              No files match "{filter}"
            </p>
          )}
        </div>
      )}

      {/* Modals */}
      {diffSnapshot && (
        <FileDiffViewer
          snapshotId={diffSnapshot.id}
          filePath={diffSnapshot.original_path}
          snapshotDate={diffSnapshot.created_at}
          onClose={() => setDiffSnapshot(null)}
        />
      )}
      {showCompare && (
        <SnapshotCompare onClose={() => setShowCompare(false)} />
      )}
      {blameFilePath && (
        <BlameView filePath={blameFilePath} onClose={() => setBlameFilePath(null)} />
      )}
    </div>
  );
}
