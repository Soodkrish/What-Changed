import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";

// Types
export interface ChangeRecord {
  id: number;
  file_id: number;
  file_path: string;
  filename: string;
  change_type: "NEW" | "MODIFIED" | "DELETED" | "MOVED";
  detected_at: string;
  previous_path: string | null;
  new_path: string | null;
}

export interface ChangeStats {
  new_count: number;
  modified_count: number;
  deleted_count: number;
  moved_count: number;
}

export interface ScanResult {
  directory: string;
  files_scanned: number;
  new_files: number;
  modified_files: number;
  total_size: number;
  errors: string[];
  scanned_at: string;
}

export interface DuplicateGroupRecord {
  id: number;
  hash: string;
  file_size: number;
  created_at: string;
  file_paths: string[];
  file_count: number;
}

export interface SnapshotRecord {
  directory: string;
  snapshot_date: string;
  total_size: number;
  file_count: number;
}

export interface MonitoredFolder {
  id: number;
  path: string;
  enabled: boolean;
  added_at: string;
}

export interface ScanBatch {
  id: number;
  folder_count: number;
  folders_scanned: string;
  started_at: string;
  completed_at: string | null;
  total_files: number;
  new_files: number;
  modified_files: number;
  deleted_files: number;
  moved_files: number;
  total_size: number;
}

export interface ScanBatchWithChanges {
  batch: ScanBatch;
  changes: ChangeRecord[];
}

// Recovery types
export interface FileSnapshotRecord {
  id: number;
  original_path: string;
  original_filename: string;
  snapshot_path: string;
  compressed_size: number;
  original_size: number;
  file_hash: string | null;
  created_at: string;
  scan_batch_id: number | null;
}

export interface RecycleBinEntry {
  id: number;
  original_path: string;
  filename: string;
  original_size: number;
  deleted_at: string;
  is_recoverable: boolean;
}

export interface CloudFolder {
  id: number;
  path: string;
  provider: string;
  display_name: string | null;
  is_active: boolean;
  detected_at: string;
}

export interface RecoveryStats {
  recycle_bin_count: number;
  snapshot_count: number;
  total_snapshot_size: number;
  cloud_folders_count: number;
}

export interface ScanProgress {
  current: number;
  total: number;
  directory: string;
  phase: string;
  progress_percent: number;
  files_scanned: number;
}

// API calls
export async function getChangesToday(): Promise<ChangeRecord[]> {
  return invoke("get_changes_today");
}

export async function getChangesRange(
  start: string,
  end: string,
): Promise<ChangeRecord[]> {
  return invoke("get_changes_range", { start, end });
}

export async function getChangeStatsToday(): Promise<ChangeStats> {
  return invoke("get_change_stats_today");
}

export async function scanDirectory(path: string): Promise<ScanResult> {
  return invoke("scan_directory", { path });
}

export async function scanAll(): Promise<ScanResult[]> {
  return invoke("scan_all");
}

export async function scanAllAsync(): Promise<void> {
  return invoke("scan_all_async");
}

export async function detectDuplicates() {
  return invoke<{ groups_found: number; wasted_bytes: number }>(
    "detect_duplicates",
  );
}

export async function getDuplicateGroups(): Promise<DuplicateGroupRecord[]> {
  return invoke("get_duplicate_groups");
}

export async function getStorageSnapshots(
  directory: string,
  days: number,
): Promise<SnapshotRecord[]> {
  return invoke("get_storage_snapshots", { directory, days });
}

export async function snapshotDirectory(path: string) {
  return invoke<{
    directory: string;
    total_size: number;
    file_count: number;
    snapshot_date: string;
  }>("snapshot_directory", { path });
}

export async function addMonitoredFolder(path: string): Promise<number> {
  return invoke("add_monitored_folder", { path });
}

export async function removeMonitoredFolder(path: string): Promise<void> {
  return invoke("remove_monitored_folder", { path });
}

export async function getMonitoredFolders(): Promise<MonitoredFolder[]> {
  return invoke("get_monitored_folders");
}

export async function toggleMonitoredFolder(
  id: number,
  enabled: boolean,
): Promise<void> {
  return invoke("toggle_monitored_folder", { id, enabled });
}

export async function getSetting(key: string): Promise<string | null> {
  return invoke("get_setting", { key });
}

export async function setSetting(
  key: string,
  value: string,
): Promise<void> {
  return invoke("set_setting", { key, value });
}

export async function getAllSettings(): Promise<Record<string, string>> {
  return invoke("get_all_settings");
}

export async function getDailySummary(): Promise<string> {
  return invoke("get_daily_summary");
}

// Native folder picker
// NOTE: Do NOT pass `filters` — on Windows, an empty filters array
// triggers "failure to get alternative strings" from the IFileOpenDialog
// COM API. For directory picking, filters are meaningless anyway.
export async function openFolderPicker(): Promise<string | null> {
  const selected = await open({
    directory: true,
    multiple: false,
    title: "Select folder to monitor",
  });
  return selected as string | null;
}

// Scan batches (commit-style grouping)
export async function getScanBatches(): Promise<ScanBatchWithChanges[]> {
  return invoke("get_scan_batches");
}

// Scheduler
export async function restartScheduler(): Promise<void> {
  return invoke("restart_scheduler");
}

export async function getSchedulerStatus(): Promise<boolean> {
  return invoke("get_scheduler_status");
}

// Auto-start
export async function enableAutostart(startMinimized?: boolean): Promise<void> {
  return invoke("enable_autostart", { startMinimized: startMinimized ?? null });
}

export async function disableAutostart(): Promise<void> {
  return invoke("disable_autostart");
}

export async function isAutostartEnabled(): Promise<boolean> {
  return invoke("is_autostart_enabled");
}

// Quit the application entirely
export async function quitApp(): Promise<void> {
  return invoke("quit_app");
}

// --- Recovery API functions ---

export async function refreshRecycleBin(): Promise<RecycleBinEntry[]> {
  return invoke("refresh_recycle_bin");
}

export async function getSnapshotContent(snapshotId: number): Promise<string> {
  return invoke("get_snapshot_content", { snapshotId });
}

export async function getFileContent(path: string): Promise<string> {
  return invoke("get_file_content", { path });
}

export async function getRecoverableFiles(): Promise<RecycleBinEntry[]> {
  return invoke("get_recoverable_files");
}

export async function restoreFromRecycleBin(entryId: number): Promise<string> {
  return invoke("restore_from_recycle_bin", { entryId });
}

export async function enableFileSnapshots(): Promise<void> {
  return invoke("enable_file_snapshots");
}

export async function disableFileSnapshots(): Promise<void> {
  return invoke("disable_file_snapshots");
}

export async function getSnapshotsForFile(path: string): Promise<FileSnapshotRecord[]> {
  return invoke("get_snapshots_for_file", { path });
}

export async function restoreFileSnapshot(snapshotId: number): Promise<string> {
  return invoke("restore_file_snapshot", { snapshotId });
}

export async function saveSnapshotToFile(snapshotId: number, destPath: string): Promise<string> {
  return invoke("save_snapshot_to_file", { snapshotId, destPath });
}

export interface SnapshotFileGroup {
  original_path: string;
  original_filename: string;
  snapshot_count: number;
  total_size: number;
  latest_snapshot: string;
  oldest_snapshot: string;
  file_exists: boolean;
}

export async function getSnapshotsGroupedByFile(): Promise<SnapshotFileGroup[]> {
  return invoke("get_snapshots_grouped_by_file");
}

export async function getSnapshotStats(): Promise<[number, number]> {
  return invoke("get_snapshot_stats");
}

export async function cleanupOldSnapshots(keepDays: number): Promise<number> {
  return invoke("cleanup_old_snapshots", { keepDays });
}

export async function detectCloudFolders(): Promise<CloudFolder[]> {
  return invoke("detect_cloud_folders");
}

export async function getCloudFolders(): Promise<CloudFolder[]> {
  return invoke("get_cloud_folders");
}

export async function isCloudBacked(path: string): Promise<string | null> {
  return invoke("is_cloud_backed", { path });
}

export async function exportDailyReport(date: string, format: string): Promise<string> {
  return invoke("export_daily_report", { date, format });
}

export async function getRecoveryStats(): Promise<RecoveryStats> {
  return invoke("get_recovery_stats");
}

// --- Ignore Patterns ---

export interface IgnorePattern {
  id: number;
  folder_id: number;
  pattern: string;
  pattern_type: string;
  created_at: string;
}

export async function addIgnorePattern(folderId: number, pattern: string, patternType: string): Promise<number> {
  return invoke("add_ignore_pattern", { folderId, pattern, patternType });
}

export async function removeIgnorePattern(id: number): Promise<void> {
  return invoke("remove_ignore_pattern", { id });
}

export async function getIgnorePatterns(folderId?: number): Promise<IgnorePattern[]> {
  return invoke("get_ignore_patterns", { folderId: folderId ?? null });
}

// --- Snapshot Tags ---

export interface SnapshotTag {
  id: number;
  snapshot_id: number;
  name: string;
  description: string | null;
  color: string;
  created_at: string;
}

export async function addSnapshotTag(snapshotId: number, name: string, description?: string, color?: string): Promise<number> {
  return invoke("add_snapshot_tag", { snapshotId, name, description: description ?? null, color: color ?? null });
}

export async function removeSnapshotTag(id: number): Promise<void> {
  return invoke("remove_snapshot_tag", { id });
}

export async function getTagsForSnapshot(snapshotId: number): Promise<SnapshotTag[]> {
  return invoke("get_tags_for_snapshot", { snapshotId });
}

export async function getAllTags(): Promise<SnapshotTag[]> {
  return invoke("get_all_tags");
}

export async function compareSnapshots(snapshotAId: number, snapshotBId: number): Promise<[string, string] | null> {
  return invoke("compare_snapshots", { snapshotAId, snapshotBId });
}

// --- Workspace Profiles ---

export interface WorkspaceProfile {
  id: number;
  name: string;
  is_active: boolean;
  created_at: string;
  updated_at: string;
}

export async function createProfile(name: string): Promise<number> {
  return invoke("create_profile", { name });
}

export async function deleteProfile(id: number): Promise<void> {
  return invoke("delete_profile", { id });
}

export async function getAllProfiles(): Promise<WorkspaceProfile[]> {
  return invoke("get_all_profiles");
}

export async function activateProfile(profileId: number): Promise<void> {
  return invoke("activate_profile", { profileId });
}

export async function saveCurrentFoldersToProfile(profileId: number): Promise<void> {
  return invoke("save_current_folders_to_profile", { profileId });
}

// --- File History ---

export async function getFileHistory(filePath: string): Promise<ChangeRecord[]> {
  return invoke("get_file_history", { filePath });
}

export async function searchChanges(query: string, limit?: number): Promise<ChangeRecord[]> {
  return invoke("search_changes", { query, limit: limit ?? null });
}

export async function getActivityHeatmap(days?: number): Promise<HeatmapEntry[]> {
  return invoke("get_activity_heatmap", { days: days ?? null });
}

export interface HeatmapEntry {
  date: string;
  count: number;
}

// --- Phase 2: Notification Profiles ---

export interface NotificationProfile {
  id: number;
  name: string;
  quiet_hours_start: number;
  quiet_hours_end: number;
  notify_new: boolean;
  notify_modified: boolean;
  notify_deleted: boolean;
  notify_moved: boolean;
  enabled: boolean;
  created_at: string;
}

export async function createNotificationProfile(name: string, quietHoursStart?: number, quietHoursEnd?: number): Promise<number> {
  return invoke("create_notification_profile", { name, quietHoursStart: quietHoursStart ?? null, quietHoursEnd: quietHoursEnd ?? null });
}

export async function deleteNotificationProfile(id: number): Promise<void> {
  return invoke("delete_notification_profile", { id });
}

export async function getAllNotificationProfiles(): Promise<NotificationProfile[]> {
  return invoke("get_all_notification_profiles");
}

export async function updateNotificationProfile(
  id: number,
  opts: {
    quiet_hours_start?: number;
    quiet_hours_end?: number;
    notify_new?: boolean;
    notify_modified?: boolean;
    notify_deleted?: boolean;
    notify_moved?: boolean;
    enabled?: boolean;
  },
): Promise<void> {
  return invoke("update_notification_profile", {
    id,
    quietHoursStart: opts.quiet_hours_start ?? null,
    quietHoursEnd: opts.quiet_hours_end ?? null,
    notifyNew: opts.notify_new ?? null,
    notifyModified: opts.notify_modified ?? null,
    notifyDeleted: opts.notify_deleted ?? null,
    notifyMoved: opts.notify_moved ?? null,
    enabled: opts.enabled ?? null,
  });
}

export async function setNotificationProfileFolders(profileId: number, folderIds: number[]): Promise<void> {
  return invoke("set_notification_profile_folders", { profileId, folderIds });
}

export async function getFoldersForNotificationProfile(profileId: number): Promise<MonitoredFolder[]> {
  return invoke("get_folders_for_notification_profile", { profileId });
}

// --- Phase 2: Webhooks ---

export interface WebhookEndpoint {
  id: number;
  name: string;
  url: string;
  events: string;
  has_secret: boolean;
  enabled: boolean;
  last_triggered: string | null;
  last_status: number | null;
  created_at: string;
}

export async function createWebhookEndpoint(name: string, url: string, events?: string, secret?: string): Promise<number> {
  return invoke("create_webhook_endpoint", { name, url, events: events ?? null, secret: secret ?? null });
}

export async function deleteWebhookEndpoint(id: number): Promise<void> {
  return invoke("delete_webhook_endpoint", { id });
}

export async function getAllWebhookEndpoints(): Promise<WebhookEndpoint[]> {
  return invoke("get_all_webhook_endpoints");
}

export async function toggleWebhookEndpoint(id: number, enabled: boolean): Promise<void> {
  return invoke("toggle_webhook_endpoint", { id, enabled });
}

export async function testWebhookEndpoint(id: number): Promise<number> {
  return invoke("test_webhook_endpoint", { id });
}

export interface WebhookFireFailure {
  endpoint_id: number;
  endpoint_name: string;
  endpoint_url: string;
  reason: string;
}

export interface WebhookFireReport {
  triggered_ids: number[];
  failures: WebhookFireFailure[];
}

export async function fireWebhookForChanges(changes: ChangeRecord[]): Promise<WebhookFireReport> {
  return invoke("fire_webhook_for_changes", { changesJson: JSON.stringify(changes) });
}

/** Run webhook diagnostics — returns a human-readable report tracing the entire pipeline. */
export async function diagnoseWebhooks(): Promise<string> {
  return invoke("diagnose_webhooks");
}

// --- Phase 2: Blame View ---

export interface BlameLine {
  line_number: number;
  content: string;
  change_type: string;
  scan_batch_id: number | null;
  detected_at: string | null;
}

export async function getBlameData(filePath: string): Promise<BlameLine[]> {
  return invoke("get_blame_data", { filePath });
}

// --- Phase 2: Changelog ---

export interface ChangelogEntry {
  date: string;
  batch_id: number;
  folders_scanned: string;
  total_files: number;
  new_files: number;
  modified_files: number;
  deleted_files: number;
  moved_files: number;
  changes: ChangeRecord[];
}

export async function getChangelogEntries(limit?: number): Promise<ChangelogEntry[]> {
  return invoke("get_changelog_entries", { limit: limit ?? null });
}

export async function generateChangelogMarkdown(limit?: number): Promise<string> {
  return invoke("generate_changelog_markdown", { limit: limit ?? null });
}

// --- Phase 2: Snapshot Compare ---

export async function compareAnySnapshots(idA: number, idB: number): Promise<[string, string, string, string] | null> {
  return invoke("compare_any_snapshots", { idA, idB });
}

// --- Phase 3: File Type Analytics ---

export interface ExtensionStat {
  extension: string;
  count: number;
  total_size: number;
}

export interface DailyTrend {
  date: string;
  new_count: number;
  modified_count: number;
  deleted_count: number;
  moved_count: number;
}

export interface AdvancedSearchResult {
  records: ChangeRecord[];
  total_count: number;
}

export interface ExportData {
  generated_at: string;
  summary: ChangeStats;
  batches: ChangelogEntry[];
  extension_stats: ExtensionStat[];
  trends: DailyTrend[];
}

export async function getExtensionStats(): Promise<ExtensionStat[]> {
  return invoke("get_extension_stats");
}

export async function getDailyTrends(days?: number): Promise<DailyTrend[]> {
  return invoke("get_daily_trends", { days: days ?? null });
}

export async function advancedSearch(opts: {
  query?: string;
  change_type?: string;
  date_from?: string;
  date_to?: string;
  extension?: string;
  min_size?: number;
  max_size?: number;
  limit?: number;
  offset?: number;
}): Promise<AdvancedSearchResult> {
  return invoke("advanced_search", {
    query: opts.query ?? null,
    changeType: opts.change_type ?? null,
    dateFrom: opts.date_from ?? null,
    dateTo: opts.date_to ?? null,
    extension: opts.extension ?? null,
    minSize: opts.min_size ?? null,
    maxSize: opts.max_size ?? null,
    limit: opts.limit ?? null,
    offset: opts.offset ?? null,
  });
}

export async function getExportData(): Promise<ExportData> {
  return invoke("get_export_data");
}

export async function exportChangesCsv(dateFrom?: string, dateTo?: string): Promise<string> {
  return invoke("export_changes_csv", { dateFrom: dateFrom ?? null, dateTo: dateTo ?? null });
}

export async function generateHtmlReport(): Promise<string> {
  return invoke("generate_html_report");
}

// Helpers
export function formatBytes(bytes: number): string {
  if (bytes === 0) return "0 B";
  const k = 1024;
  const sizes = ["B", "KB", "MB", "GB", "TB"];
  const i = Math.floor(Math.log(bytes) / Math.log(k));
  return parseFloat((bytes / Math.pow(k, i)).toFixed(2)) + " " + sizes[i];
}

/**
 * Parse a timestamp string from the database.
 * SQLite CURRENT_TIMESTAMP returns bare UTC like "2026-07-13 10:30:00" — no timezone.
 * JavaScript's new Date() interprets those as LOCAL time, causing a timezone offset bug.
 * This function appends "Z" (UTC) when no timezone indicator is present.
 * Timestamps from chrono (with +00:00 or Z) are left untouched.
 */
export function parseDbTimestamp(dateStr: string): Date {
  if (!dateStr) return new Date(0);
  // If the string already has a timezone suffix (Z, +HH:MM, -HH:MM), use as-is
  if (/[Zz]|[+-]\d{2}:\d{2}$/.test(dateStr)) {
    return new Date(dateStr);
  }
  // Bare UTC timestamp — append Z so JS parses it as UTC, not local
  return new Date(dateStr + "Z");
}

export function timeAgo(dateStr: string): string {
  const date = parseDbTimestamp(dateStr);
  const now = new Date();
  const seconds = Math.floor((now.getTime() - date.getTime()) / 1000);

  if (seconds < 60) return "just now";
  if (seconds < 3600) return `${Math.floor(seconds / 60)}m ago`;
  if (seconds < 86400) return `${Math.floor(seconds / 3600)}h ago`;
  return `${Math.floor(seconds / 86400)}d ago`;
}

// --- Update Checker ---

export interface UpdateInfo {
  has_update: boolean;
  current_version: string;
  latest_version: string;
  download_url: string;
  release_notes: string;
}

export async function checkForUpdates(): Promise<UpdateInfo> {
  return invoke("check_for_updates");
}
