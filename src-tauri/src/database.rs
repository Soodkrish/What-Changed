use rusqlite::{params, Connection, Result};
use std::sync::Mutex;
use std::path::Path;

pub struct Database {
    pub conn: Mutex<Connection>,
}

impl Database {
    pub fn new(db_path: &Path) -> Result<Self> {
        let conn = Connection::open(db_path)?;

        // --- Memory optimization pragmas ---
        conn.execute_batch(
            "
            PRAGMA journal_mode = WAL;
            PRAGMA synchronous = NORMAL;
            PRAGMA cache_size = -2000;
            PRAGMA mmap_size = 268435456;
            PRAGMA temp_store = MEMORY;
            PRAGMA busy_timeout = 5000;
            PRAGMA page_size = 4096;
            PRAGMA auto_vacuum = INCREMENTAL;
            ",
        )?;

        let db = Database {
            conn: Mutex::new(conn),
        };
        db.init_tables()?;
        Ok(db)
    }

    fn init_tables(&self) -> Result<()> {
        let conn = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        conn.execute_batch(
            "
            CREATE TABLE IF NOT EXISTS files (
                id INTEGER PRIMARY KEY,
                path TEXT UNIQUE NOT NULL,
                filename TEXT NOT NULL,
                extension TEXT,
                size INTEGER NOT NULL,
                mtime DATETIME NOT NULL,
                ctime DATETIME NOT NULL,
                hash TEXT,
                first_seen DATETIME DEFAULT CURRENT_TIMESTAMP,
                last_seen DATETIME DEFAULT CURRENT_TIMESTAMP,
                is_deleted BOOLEAN DEFAULT 0
            );

            CREATE TABLE IF NOT EXISTS changes (
                id INTEGER PRIMARY KEY,
                file_id INTEGER REFERENCES files(id),
                change_type TEXT CHECK(change_type IN ('NEW', 'MODIFIED', 'DELETED', 'MOVED')),
                detected_at DATETIME DEFAULT CURRENT_TIMESTAMP,
                previous_path TEXT,
                new_path TEXT
            );

            CREATE TABLE IF NOT EXISTS snapshots (
                id INTEGER PRIMARY KEY,
                directory TEXT NOT NULL,
                snapshot_date DATE NOT NULL,
                total_size INTEGER NOT NULL,
                file_count INTEGER NOT NULL,
                UNIQUE(directory, snapshot_date)
            );

            CREATE TABLE IF NOT EXISTS duplicate_groups (
                id INTEGER PRIMARY KEY,
                hash TEXT NOT NULL,
                file_size INTEGER NOT NULL,
                created_at DATETIME DEFAULT CURRENT_TIMESTAMP
            );

            CREATE TABLE IF NOT EXISTS duplicate_files (
                group_id INTEGER REFERENCES duplicate_groups(id),
                file_id INTEGER REFERENCES files(id),
                PRIMARY KEY (group_id, file_id)
            );

            CREATE TABLE IF NOT EXISTS settings (
                key TEXT PRIMARY KEY,
                value TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS monitored_folders (
                id INTEGER PRIMARY KEY,
                path TEXT UNIQUE NOT NULL,
                enabled BOOLEAN DEFAULT 1,
                added_at DATETIME DEFAULT CURRENT_TIMESTAMP
            );

            CREATE TABLE IF NOT EXISTS scan_batches (
                id INTEGER PRIMARY KEY,
                folder_count INTEGER NOT NULL,
                folders_scanned TEXT DEFAULT '',
                started_at DATETIME DEFAULT CURRENT_TIMESTAMP,
                completed_at DATETIME,
                total_files INTEGER DEFAULT 0,
                new_files INTEGER DEFAULT 0,
                modified_files INTEGER DEFAULT 0,
                deleted_files INTEGER DEFAULT 0,
                moved_files INTEGER DEFAULT 0,
                total_size INTEGER DEFAULT 0
            );

            CREATE INDEX IF NOT EXISTS idx_files_path ON files(path);
            CREATE INDEX IF NOT EXISTS idx_files_hash ON files(hash);
            CREATE INDEX IF NOT EXISTS idx_changes_file ON changes(file_id);
            CREATE INDEX IF NOT EXISTS idx_changes_time ON changes(detected_at);
            CREATE INDEX IF NOT EXISTS idx_snapshots_dir_date ON snapshots(directory, snapshot_date);
            ",
        )?;

        // Migration: add new_path column if it doesn't exist
        let has_new_path: bool = conn
            .prepare("SELECT new_path FROM changes LIMIT 1")
            .is_ok();
        if !has_new_path {
            conn.execute_batch("ALTER TABLE changes ADD COLUMN new_path TEXT;")
                .ok();
        }

        // Migration: add scan_batch_id column if it doesn't exist
        let has_batch_id: bool = conn
            .prepare("SELECT scan_batch_id FROM changes LIMIT 1")
            .is_ok();
        if !has_batch_id {
            conn.execute_batch("ALTER TABLE changes ADD COLUMN scan_batch_id INTEGER;")
                .ok();
            conn.execute_batch("CREATE INDEX IF NOT EXISTS idx_changes_batch ON changes(scan_batch_id);")
                .ok();
        }

        // Migration: add folders_scanned column if it doesn't exist
        let has_folders_scanned: bool = conn
            .prepare("SELECT folders_scanned FROM scan_batches LIMIT 1")
            .is_ok();
        if !has_folders_scanned {
            conn.execute_batch("ALTER TABLE scan_batches ADD COLUMN folders_scanned TEXT DEFAULT '';")
                .ok();
        }

        // --- Recovery feature tables ---
        conn.execute_batch(
            "
            CREATE TABLE IF NOT EXISTS file_snapshots (
                id INTEGER PRIMARY KEY,
                original_path TEXT NOT NULL,
                original_filename TEXT NOT NULL,
                snapshot_path TEXT NOT NULL,
                compressed_size INTEGER NOT NULL,
                original_size INTEGER NOT NULL,
                file_hash TEXT,
                created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
                scan_batch_id INTEGER
            );

            CREATE TABLE IF NOT EXISTS recycle_bin_entries (
                id INTEGER PRIMARY KEY,
                original_path TEXT UNIQUE NOT NULL,
                filename TEXT NOT NULL,
                original_size INTEGER NOT NULL,
                deleted_at DATETIME NOT NULL,
                is_recoverable BOOLEAN DEFAULT 1,
                last_checked DATETIME DEFAULT CURRENT_TIMESTAMP
            );

            CREATE TABLE IF NOT EXISTS cloud_folders (
                id INTEGER PRIMARY KEY,
                path TEXT UNIQUE NOT NULL,
                provider TEXT NOT NULL,
                display_name TEXT,
                is_active BOOLEAN DEFAULT 1,
                detected_at DATETIME DEFAULT CURRENT_TIMESTAMP
            );

            CREATE TABLE IF NOT EXISTS recovery_audit_log (
                id INTEGER PRIMARY KEY,
                action TEXT NOT NULL,
                details TEXT,
                performed_at DATETIME DEFAULT CURRENT_TIMESTAMP,
                success BOOLEAN DEFAULT 1,
                error_message TEXT
            );

            CREATE INDEX IF NOT EXISTS idx_fs_path ON file_snapshots(original_path);
            CREATE INDEX IF NOT EXISTS idx_fs_hash ON file_snapshots(file_hash);
            CREATE INDEX IF NOT EXISTS idx_rb_path ON recycle_bin_entries(original_path);
            CREATE INDEX IF NOT EXISTS idx_rb_last_checked ON recycle_bin_entries(last_checked);
            CREATE INDEX IF NOT EXISTS idx_ral_performed ON recovery_audit_log(performed_at);
            ",
        )?;

        // --- Phase 1 features: Ignore Patterns, Snapshot Tags, Workspace Profiles ---
        conn.execute_batch(
            "
            CREATE TABLE IF NOT EXISTS ignore_patterns (
                id INTEGER PRIMARY KEY,
                folder_id INTEGER REFERENCES monitored_folders(id) ON DELETE CASCADE,
                pattern TEXT NOT NULL,
                pattern_type TEXT NOT NULL DEFAULT 'glob',
                created_at DATETIME DEFAULT CURRENT_TIMESTAMP
            );

            CREATE TABLE IF NOT EXISTS snapshot_tags (
                id INTEGER PRIMARY KEY,
                snapshot_id INTEGER REFERENCES file_snapshots(id) ON DELETE CASCADE,
                name TEXT NOT NULL,
                description TEXT,
                color TEXT DEFAULT '#6366f1',
                created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
                UNIQUE(snapshot_id, name)
            );

            CREATE TABLE IF NOT EXISTS workspace_profiles (
                id INTEGER PRIMARY KEY,
                name TEXT UNIQUE NOT NULL,
                is_active BOOLEAN DEFAULT 0,
                created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
                updated_at DATETIME DEFAULT CURRENT_TIMESTAMP
            );

            CREATE TABLE IF NOT EXISTS profile_folders (
                profile_id INTEGER REFERENCES workspace_profiles(id) ON DELETE CASCADE,
                folder_id INTEGER REFERENCES monitored_folders(id) ON DELETE CASCADE,
                PRIMARY KEY (profile_id, folder_id)
            );

            CREATE INDEX IF NOT EXISTS idx_ip_folder ON ignore_patterns(folder_id);
            CREATE INDEX IF NOT EXISTS idx_st_snapshot ON snapshot_tags(snapshot_id);
            CREATE INDEX IF NOT EXISTS idx_st_name ON snapshot_tags(name);
            CREATE INDEX IF NOT EXISTS idx_pf_profile ON profile_folders(profile_id);
            CREATE INDEX IF NOT EXISTS idx_pf_folder ON profile_folders(folder_id);
            ",
        )?;

        // --- Phase 2: Notification Profiles, Webhooks ---
        conn.execute_batch(
            "
            CREATE TABLE IF NOT EXISTS notification_profiles (
                id INTEGER PRIMARY KEY,
                name TEXT UNIQUE NOT NULL,
                quiet_hours_start INTEGER DEFAULT 0,
                quiet_hours_end INTEGER DEFAULT 0,
                notify_new BOOLEAN DEFAULT 1,
                notify_modified BOOLEAN DEFAULT 1,
                notify_deleted BOOLEAN DEFAULT 1,
                notify_moved BOOLEAN DEFAULT 1,
                enabled BOOLEAN DEFAULT 1,
                created_at DATETIME DEFAULT CURRENT_TIMESTAMP
            );

            CREATE TABLE IF NOT EXISTS notification_profile_folders (
                profile_id INTEGER REFERENCES notification_profiles(id) ON DELETE CASCADE,
                folder_id INTEGER REFERENCES monitored_folders(id) ON DELETE CASCADE,
                PRIMARY KEY (profile_id, folder_id)
            );

            CREATE TABLE IF NOT EXISTS webhook_endpoints (
                id INTEGER PRIMARY KEY,
                name TEXT NOT NULL,
                url TEXT NOT NULL,
                events TEXT DEFAULT 'ALL',
                secret TEXT,
                enabled BOOLEAN DEFAULT 1,
                last_triggered DATETIME,
                last_status INTEGER,
                created_at DATETIME DEFAULT CURRENT_TIMESTAMP
            );

            CREATE INDEX IF NOT EXISTS idx_np_name ON notification_profiles(name);
            CREATE INDEX IF NOT EXISTS idx_npf_profile ON notification_profile_folders(profile_id);
            CREATE INDEX IF NOT EXISTS idx_wh_url ON webhook_endpoints(url);
            ",
        )?;

        // --- Startup cleanup: prune old scan batches (keep last 100) ---
        conn.execute(
            "DELETE FROM scan_batches WHERE id NOT IN (SELECT id FROM scan_batches ORDER BY id DESC LIMIT 100)",
            [],
        )?;
        // --- Startup cleanup: prune old audit logs (keep 90 days) ---
        conn.execute(
            "DELETE FROM recovery_audit_log WHERE performed_at < datetime('now', '-90 days')",
            [],
        ).ok();

        // --- Let SQLite analyze its indexes for optimal query planning ---
        conn.execute_batch("PRAGMA optimize;")?;

        Ok(())
    }

    // --- File operations ---

    pub fn upsert_file(
        &self,
        path: &str,
        filename: &str,
        extension: Option<&str>,
        size: i64,
        mtime: &str,
        ctime: &str,
    ) -> Result<i64> {
        let conn = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        conn.execute(
            "INSERT INTO files (path, filename, extension, size, mtime, ctime)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)
             ON CONFLICT(path) DO UPDATE SET
                size = excluded.size,
                mtime = excluded.mtime,
                last_seen = CURRENT_TIMESTAMP,
                is_deleted = 0",
            params![path, filename, extension, size, mtime, ctime],
        )?;
        Ok(conn.last_insert_rowid())
    }

    pub fn mark_file_deleted(&self, path: &str) -> Result<()> {
        let conn = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        conn.execute(
            "UPDATE files SET is_deleted = 1 WHERE path = ?1",
            params![path],
        )?;
        Ok(())
    }

    pub fn get_file_by_path(&self, path: &str) -> Result<Option<FileRecord>> {
        let conn = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        let mut stmt = conn.prepare(
            "SELECT id, path, filename, extension, size, mtime, ctime, hash, first_seen, last_seen, is_deleted
             FROM files WHERE path = ?1",
        )?;
        let mut rows = stmt.query_map(params![path], |row| {
            Ok(FileRecord {
                id: row.get(0)?,
                path: row.get(1)?,
                filename: row.get(2)?,
                extension: row.get(3)?,
                size: row.get(4)?,
                mtime: row.get(5)?,
                ctime: row.get(6)?,
                hash: row.get(7)?,
                first_seen: row.get(8)?,
                last_seen: row.get(9)?,
                is_deleted: row.get(10)?,
            })
        })?;
        match rows.next() {
            Some(row) => Ok(Some(row?)),
            None => Ok(None),
        }
    }

    /// Batch query: get deleted files matching any of the given paths (single query, not N+1)
    pub fn get_deleted_files_by_paths(&self, paths: &[&str]) -> Result<Vec<FileRecord>> {
        if paths.is_empty() {
            return Ok(Vec::new());
        }
        let conn = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        // Build parameterized IN clause
        let placeholders: Vec<String> = (0..paths.len()).map(|i| format!("?{}", i + 1)).collect();
        let sql = format!(
            "SELECT id, path, filename, extension, size, mtime, ctime, hash, first_seen, last_seen, is_deleted
             FROM files WHERE path IN ({}) AND is_deleted = 1",
            placeholders.join(",")
        );
        let mut stmt = conn.prepare(&sql)?;
        let params: Vec<&dyn rusqlite::types::ToSql> = paths.iter().map(|p| p as &dyn rusqlite::types::ToSql).collect();
        let rows = stmt.query_map(params.as_slice(), |row| {
            Ok(FileRecord {
                id: row.get(0)?,
                path: row.get(1)?,
                filename: row.get(2)?,
                extension: row.get(3)?,
                size: row.get(4)?,
                mtime: row.get(5)?,
                ctime: row.get(6)?,
                hash: row.get(7)?,
                first_seen: row.get(8)?,
                last_seen: row.get(9)?,
                is_deleted: row.get(10)?,
            })
        })?;
        rows.collect::<Result<Vec<_>>>()
    }

    pub fn get_all_active_files(&self) -> Result<Vec<FileRecord>> {
        let conn = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        let mut stmt = conn.prepare(
            "SELECT id, path, filename, extension, size, mtime, ctime, hash, first_seen, last_seen, is_deleted
             FROM files WHERE is_deleted = 0",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(FileRecord {
                id: row.get(0)?,
                path: row.get(1)?,
                filename: row.get(2)?,
                extension: row.get(3)?,
                size: row.get(4)?,
                mtime: row.get(5)?,
                ctime: row.get(6)?,
                hash: row.get(7)?,
                first_seen: row.get(8)?,
                last_seen: row.get(9)?,
                is_deleted: row.get(10)?,
            })
        })?;
        rows.collect::<Result<Vec<_>>>()
    }

    pub fn update_file_hash(&self, id: i64, hash: &str) -> Result<()> {
        let conn = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        conn.execute(
            "UPDATE files SET hash = ?1 WHERE id = ?2",
            params![hash, id],
        )?;
        Ok(())
    }

    // --- Change operations ---

    pub fn insert_change(
        &self,
        file_id: i64,
        change_type: &str,
        previous_path: Option<&str>,
    ) -> Result<i64> {
        let conn = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        conn.execute(
            "INSERT INTO changes (file_id, change_type, previous_path, scan_batch_id)
             VALUES (?1, ?2, ?3, (SELECT MAX(id) FROM scan_batches))",
            params![file_id, change_type, previous_path],
        )?;
        Ok(conn.last_insert_rowid())
    }

    pub fn insert_change_with_paths(
        &self,
        file_id: i64,
        change_type: &str,
        previous_path: Option<&str>,
        new_path: Option<&str>,
    ) -> Result<i64> {
        let conn = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        conn.execute(
            "INSERT INTO changes (file_id, change_type, previous_path, new_path, scan_batch_id)
             VALUES (?1, ?2, ?3, ?4, (SELECT MAX(id) FROM scan_batches))",
            params![file_id, change_type, previous_path, new_path],
        )?;
        Ok(conn.last_insert_rowid())
    }

    pub fn insert_change_in_batch(
        &self,
        file_id: i64,
        change_type: &str,
        previous_path: Option<&str>,
        new_path: Option<&str>,
        batch_id: i64,
    ) -> Result<i64> {
        let conn = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        conn.execute(
            "INSERT INTO changes (file_id, change_type, previous_path, new_path, scan_batch_id)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![file_id, change_type, previous_path, new_path, batch_id],
        )?;
        Ok(conn.last_insert_rowid())
    }

    // --- Scan batch operations ---

    pub fn create_scan_batch(&self, folder_count: i64, folders_scanned: &str) -> Result<i64> {
        let conn = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        conn.execute(
            "INSERT INTO scan_batches (folder_count, started_at, folders_scanned) VALUES (?1, CURRENT_TIMESTAMP, ?2)",
            params![folder_count, folders_scanned],
        )?;
        Ok(conn.last_insert_rowid())
    }

    pub fn complete_scan_batch(
        &self,
        batch_id: i64,
        total_files: i64,
        new_files: i64,
        modified_files: i64,
        deleted_files: i64,
        moved_files: i64,
        total_size: i64,
    ) -> Result<()> {
        let conn = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        conn.execute(
            "UPDATE scan_batches SET
                completed_at = CURRENT_TIMESTAMP,
                total_files = ?2,
                new_files = ?3,
                modified_files = ?4,
                deleted_files = ?5,
                moved_files = ?6,
                total_size = ?7
             WHERE id = ?1",
            params![batch_id, total_files, new_files, modified_files, deleted_files, moved_files, total_size],
        )?;
        Ok(())
    }

    pub fn get_scan_batches_today(&self) -> Result<Vec<ScanBatch>> {
        let conn = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        let mut stmt = conn.prepare(
            "SELECT id, folder_count, COALESCE(folders_scanned, ''), started_at, completed_at,
                    total_files, new_files, modified_files, deleted_files, moved_files, total_size
             FROM scan_batches
             WHERE DATE(started_at) = DATE('now')
             ORDER BY started_at DESC",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(ScanBatch {
                id: row.get(0)?,
                folder_count: row.get(1)?,
                folders_scanned: row.get(2)?,
                started_at: row.get(3)?,
                completed_at: row.get(4)?,
                total_files: row.get(5).unwrap_or(0),
                new_files: row.get(6).unwrap_or(0),
                modified_files: row.get(7).unwrap_or(0),
                deleted_files: row.get(8).unwrap_or(0),
                moved_files: row.get(9).unwrap_or(0),
                total_size: row.get(10).unwrap_or(0),
            })
        })?;
        rows.collect::<Result<Vec<_>>>()
    }

    pub fn get_latest_scan_batch(&self) -> Option<ScanBatch> {
        let conn = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        let mut stmt = conn.prepare(
            "SELECT id, folder_count, COALESCE(folders_scanned, ''), started_at, completed_at,
                    total_files, new_files, modified_files, deleted_files, moved_files, total_size
             FROM scan_batches
             ORDER BY id DESC LIMIT 1",
        ).ok()?;
        let mut rows = stmt.query_map([], |row| {
            Ok(ScanBatch {
                id: row.get(0)?,
                folder_count: row.get(1)?,
                folders_scanned: row.get(2)?,
                started_at: row.get(3)?,
                completed_at: row.get(4)?,
                total_files: row.get(5).unwrap_or(0),
                new_files: row.get(6).unwrap_or(0),
                modified_files: row.get(7).unwrap_or(0),
                deleted_files: row.get(8).unwrap_or(0),
                moved_files: row.get(9).unwrap_or(0),
                total_size: row.get(10).unwrap_or(0),
            })
        }).ok()?;
        rows.next()?.ok()
    }

    pub fn get_changes_in_batch(&self, batch_id: i64) -> Result<Vec<ChangeRecord>> {
        let conn = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        let mut stmt = conn.prepare(
            "SELECT c.id, c.file_id, f.path, f.filename, c.change_type, c.detected_at, c.previous_path, c.new_path
             FROM changes c
             JOIN files f ON c.file_id = f.id
             WHERE c.scan_batch_id = ?1
             ORDER BY c.detected_at DESC",
        )?;
        let rows = stmt.query_map(params![batch_id], |row| {
            Ok(ChangeRecord {
                id: row.get(0)?,
                file_id: row.get(1)?,
                file_path: row.get(2)?,
                filename: row.get(3)?,
                change_type: row.get(4)?,
                detected_at: row.get(5)?,
                previous_path: row.get(6)?,
                new_path: row.get(7)?,
            })
        })?;
        rows.collect::<Result<Vec<_>>>()
    }

    pub fn get_all_batches_with_changes(&self) -> Result<Vec<ScanBatchWithChanges>> {
        let conn = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        let mut stmt = conn.prepare(
            "SELECT sb.id, sb.folder_count, COALESCE(sb.folders_scanned, ''), sb.started_at, sb.completed_at,
                    sb.total_files, sb.new_files, sb.modified_files, sb.deleted_files, sb.moved_files, sb.total_size,
                    c.id, c.file_id, c.change_type, c.detected_at, c.previous_path, c.new_path,
                    f.path, f.filename
             FROM scan_batches sb
             LEFT JOIN changes c ON sb.id = c.scan_batch_id
             LEFT JOIN files f ON c.file_id = f.id
             WHERE DATE(sb.started_at) = DATE('now')
             ORDER BY sb.started_at DESC, c.detected_at DESC",
        )?;

        let mut batch_map: std::collections::HashMap<i64, ScanBatchWithChanges> = std::collections::HashMap::new();

        let rows = stmt.query_map([], |row| {
            let batch = ScanBatch {
                id: row.get(0)?,
                folder_count: row.get(1)?,
                folders_scanned: row.get(2)?,
                started_at: row.get(3)?,
                completed_at: row.get(4)?,
                total_files: row.get(5)?,
                new_files: row.get(6)?,
                modified_files: row.get(7)?,
                deleted_files: row.get(8)?,
                moved_files: row.get(9)?,
                total_size: row.get(10)?,
            };
            let change_id: Option<i64> = row.get(11)?;
            let change = if let Some(cid) = change_id {
                Some(ChangeRecord {
                    id: cid,
                    file_id: row.get(12)?,
                    file_path: row.get(17)?,
                    filename: row.get(18)?,
                    change_type: row.get(13)?,
                    detected_at: row.get(14)?,
                    previous_path: row.get(15)?,
                    new_path: row.get(16)?,
                })
            } else {
                None
            };
            Ok((batch, change))
        })?;

        for row in rows {
            let (batch, change) = row?;
            let entry = batch_map.entry(batch.id).or_insert_with(|| ScanBatchWithChanges {
                batch,
                changes: Vec::new(),
            });
            if let Some(c) = change {
                entry.changes.push(c);
            }
        }

        let mut result: Vec<ScanBatchWithChanges> = batch_map.into_values().collect();
        result.sort_by(|a, b| b.batch.started_at.cmp(&a.batch.started_at));
        Ok(result)
    }

    /// Find active files matching a given filename and size (for move detection).
    /// Excludes a specific path from results.
    pub fn find_file_by_name_and_size(
        &self,
        filename: &str,
        size: i64,
        exclude_path: &str,
    ) -> Result<Vec<FileRecord>> {
        let conn = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        let mut stmt = conn.prepare(
            "SELECT id, path, filename, extension, size, mtime, ctime, hash, first_seen, last_seen, is_deleted
             FROM files
             WHERE is_deleted = 0 AND filename = ?1 AND size = ?2 AND path != ?3",
        )?;
        let rows = stmt.query_map(params![filename, size, exclude_path], |row| {
            Ok(FileRecord {
                id: row.get(0)?,
                path: row.get(1)?,
                filename: row.get(2)?,
                extension: row.get(3)?,
                size: row.get(4)?,
                mtime: row.get(5)?,
                ctime: row.get(6)?,
                hash: row.get(7)?,
                first_seen: row.get(8)?,
                last_seen: row.get(9)?,
                is_deleted: row.get(10)?,
            })
        })?;
        rows.collect::<Result<Vec<_>>>()
    }

    /// Update a file record's path (for move detection — file renamed/moved)
    pub fn update_file_path(&self, old_path: &str, new_path: &str) -> Result<()> {
        let conn = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        conn.execute(
            "UPDATE files SET path = ?1, last_seen = CURRENT_TIMESTAMP WHERE path = ?2",
            params![new_path, old_path],
        )?;
        Ok(())
    }

    pub fn get_changes_today(&self) -> Result<Vec<ChangeRecord>> {
        let conn = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        let mut stmt = conn.prepare(
            "SELECT c.id, c.file_id, f.path, f.filename, c.change_type, c.detected_at, c.previous_path, c.new_path
             FROM changes c
             JOIN files f ON c.file_id = f.id
             WHERE DATE(c.detected_at) = DATE('now')
             ORDER BY c.detected_at DESC",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(ChangeRecord {
                id: row.get(0)?,
                file_id: row.get(1)?,
                file_path: row.get(2)?,
                filename: row.get(3)?,
                change_type: row.get(4)?,
                detected_at: row.get(5)?,
                previous_path: row.get(6)?,
                new_path: row.get(7)?,
            })
        })?;
        rows.collect::<Result<Vec<_>>>()
    }

    pub fn get_changes_range(&self, start: &str, end: &str) -> Result<Vec<ChangeRecord>> {
        let conn = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        let mut stmt = conn.prepare(
            "SELECT c.id, c.file_id, f.path, f.filename, c.change_type, c.detected_at, c.previous_path, c.new_path
             FROM changes c
             JOIN files f ON c.file_id = f.id
             WHERE c.detected_at >= ?1 AND c.detected_at <= ?2
             ORDER BY c.detected_at DESC",
        )?;
        let rows = stmt.query_map(params![start, end], |row| {
            Ok(ChangeRecord {
                id: row.get(0)?,
                file_id: row.get(1)?,
                file_path: row.get(2)?,
                filename: row.get(3)?,
                change_type: row.get(4)?,
                detected_at: row.get(5)?,
                previous_path: row.get(6)?,
                new_path: row.get(7)?,
            })
        })?;
        rows.collect::<Result<Vec<_>>>()
    }

    pub fn get_change_stats_today(&self) -> Result<ChangeStats> {
        let conn = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        let mut stmt = conn.prepare(
            "SELECT change_type, COUNT(*) FROM changes WHERE DATE(detected_at) = DATE('now') GROUP BY change_type",
        )?;
        let mut stats = ChangeStats {
            new_count: 0,
            modified_count: 0,
            deleted_count: 0,
            moved_count: 0,
        };
        let rows = stmt.query_map([], |row| {
            let change_type: String = row.get(0)?;
            let count: i64 = row.get(1)?;
            Ok((change_type, count))
        })?;
        for row in rows {
            let (change_type, count) = row?;
            match change_type.as_str() {
                "NEW" => stats.new_count = count,
                "MODIFIED" => stats.modified_count = count,
                "DELETED" => stats.deleted_count = count,
                "MOVED" => stats.moved_count = count,
                _ => {}
            }
        }
        Ok(stats)
    }

    // --- Snapshot operations ---

    pub fn insert_snapshot(
        &self,
        directory: &str,
        snapshot_date: &str,
        total_size: i64,
        file_count: i64,
    ) -> Result<()> {
        let conn = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        conn.execute(
            "INSERT OR REPLACE INTO snapshots (directory, snapshot_date, total_size, file_count)
             VALUES (?1, ?2, ?3, ?4)",
            params![directory, snapshot_date, total_size, file_count],
        )?;
        Ok(())
    }

    pub fn get_snapshots(&self, directory: &str, days: i64) -> Result<Vec<SnapshotRecord>> {
        let conn = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        let mut stmt = conn.prepare(
            "SELECT directory, snapshot_date, total_size, file_count
             FROM snapshots
             WHERE directory = ?1 AND snapshot_date >= DATE('now', '-' || ?2 || ' days')
             ORDER BY snapshot_date ASC",
        )?;
        let rows = stmt.query_map(params![directory, days], |row| {
            Ok(SnapshotRecord {
                directory: row.get(0)?,
                snapshot_date: row.get(1)?,
                total_size: row.get(2)?,
                file_count: row.get(3)?,
            })
        })?;
        rows.collect::<Result<Vec<_>>>()
    }

    // --- Duplicate operations ---

    pub fn clear_duplicates(&self) -> Result<()> {
        let conn = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        conn.execute("DELETE FROM duplicate_files", [])?;
        conn.execute("DELETE FROM duplicate_groups", [])?;
        Ok(())
    }

    pub fn insert_duplicate_group(&self, hash: &str, file_size: i64) -> Result<i64> {
        let conn = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        conn.execute(
            "INSERT INTO duplicate_groups (hash, file_size) VALUES (?1, ?2)",
            params![hash, file_size],
        )?;
        Ok(conn.last_insert_rowid())
    }

    pub fn insert_duplicate_file(&self, group_id: i64, file_id: i64) -> Result<()> {
        let conn = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        conn.execute(
            "INSERT OR IGNORE INTO duplicate_files (group_id, file_id) VALUES (?1, ?2)",
            params![group_id, file_id],
        )?;
        Ok(())
    }

    pub fn get_duplicate_groups(&self) -> Result<Vec<DuplicateGroupRecord>> {
        let conn = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        let mut stmt = conn.prepare(
            "SELECT dg.id, dg.hash, dg.file_size, dg.created_at,
                    GROUP_CONCAT(f.path, '|') as file_paths,
                    COUNT(df.file_id) as file_count
             FROM duplicate_groups dg
             JOIN duplicate_files df ON dg.id = df.group_id
             JOIN files f ON df.file_id = f.id
             GROUP BY dg.id
             HAVING COUNT(df.file_id) > 1
             ORDER BY dg.file_size DESC",
        )?;
        let rows = stmt.query_map([], |row| {
            // SQL columns: 0=id, 1=hash, 2=file_size, 3=created_at, 4=GROUP_CONCAT(paths), 5=COUNT
            let paths_str: String = row.get(4)?;
            let paths: Vec<String> = paths_str.split('|').map(|s| s.to_string()).collect();
            Ok(DuplicateGroupRecord {
                id: row.get(0)?,
                hash: row.get(1)?,
                file_size: row.get(2)?,
                created_at: row.get(3)?,
                file_paths: paths,
                file_count: row.get(5)?,
            })
        })?;
        rows.collect::<Result<Vec<_>>>()
    }

    // --- Settings ---

    pub fn get_setting(&self, key: &str) -> Result<Option<String>> {
        let conn = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        let mut stmt = conn.prepare("SELECT value FROM settings WHERE key = ?1")?;
        let mut rows = stmt.query_map(params![key], |row| row.get::<_, String>(0))?;
        match rows.next() {
            Some(row) => Ok(Some(row?)),
            None => Ok(None),
        }
    }

    pub fn set_setting(&self, key: &str, value: &str) -> Result<()> {
        let conn = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        conn.execute(
            "INSERT OR REPLACE INTO settings (key, value) VALUES (?1, ?2)",
            params![key, value],
        )?;
        Ok(())
    }

    pub fn get_all_settings(&self) -> Result<std::collections::HashMap<String, String>> {
        let conn = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        let mut stmt = conn.prepare("SELECT key, value FROM settings")?;
        let mut settings = std::collections::HashMap::new();
        let rows = stmt.query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?;
        for row in rows {
            let (key, value) = row?;
            settings.insert(key, value);
        }
        Ok(settings)
    }

    // --- Monitored Folders ---

    pub fn add_monitored_folder(&self, path: &str) -> Result<i64> {
        let conn = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        conn.execute(
            "INSERT OR IGNORE INTO monitored_folders (path) VALUES (?1)",
            params![path],
        )?;
        Ok(conn.last_insert_rowid())
    }

    pub fn remove_monitored_folder(&self, path: &str) -> Result<()> {
        let conn = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        conn.execute(
            "DELETE FROM monitored_folders WHERE path = ?1",
            params![path],
        )?;
        Ok(())
    }

    pub fn get_monitored_folders(&self) -> Result<Vec<MonitoredFolder>> {
        let conn = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        let mut stmt = conn.prepare(
            "SELECT id, path, enabled, added_at FROM monitored_folders ORDER BY added_at",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(MonitoredFolder {
                id: row.get(0)?,
                path: row.get(1)?,
                enabled: row.get(2)?,
                added_at: row.get(3)?,
            })
        })?;
        rows.collect::<Result<Vec<_>>>()
    }

    pub fn toggle_monitored_folder(&self, id: i64, enabled: bool) -> Result<()> {
        let conn = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        conn.execute(
            "UPDATE monitored_folders SET enabled = ?1 WHERE id = ?2",
            params![enabled, id],
        )?;
        Ok(())
    }
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct FileRecord {
    pub id: i64,
    pub path: String,
    pub filename: String,
    pub extension: Option<String>,
    pub size: i64,
    pub mtime: String,
    pub ctime: String,
    pub hash: Option<String>,
    pub first_seen: String,
    pub last_seen: String,
    pub is_deleted: bool,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ChangeRecord {
    pub id: i64,
    #[serde(default)]
    pub file_id: i64,
    #[serde(default)]
    pub file_path: String,
    #[serde(default)]
    pub filename: String,
    #[serde(default)]
    pub change_type: String,
    #[serde(default)]
    pub detected_at: String,
    #[serde(default)]
    pub previous_path: Option<String>,
    #[serde(default)]
    pub new_path: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize, Default)]
pub struct ChangeStats {
    pub new_count: i64,
    pub modified_count: i64,
    pub deleted_count: i64,
    pub moved_count: i64,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct SnapshotRecord {
    pub directory: String,
    pub snapshot_date: String,
    pub total_size: i64,
    pub file_count: i64,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct DuplicateGroupRecord {
    pub id: i64,
    pub hash: String,
    pub file_size: i64,
    pub created_at: String,
    pub file_paths: Vec<String>,
    pub file_count: i64,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct MonitoredFolder {
    pub id: i64,
    pub path: String,
    pub enabled: bool,
    pub added_at: String,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct ScanBatch {
    pub id: i64,
    pub folder_count: i64,
    pub folders_scanned: String,
    pub started_at: String,
    pub completed_at: Option<String>,
    pub total_files: i64,
    pub new_files: i64,
    pub modified_files: i64,
    pub deleted_files: i64,
    pub moved_files: i64,
    pub total_size: i64,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct ScanBatchWithChanges {
    pub batch: ScanBatch,
    pub changes: Vec<ChangeRecord>,
}

// --- Recovery feature structs ---

#[derive(Debug, Clone, serde::Serialize)]
pub struct FileSnapshotRecord {
    pub id: i64,
    pub original_path: String,
    pub original_filename: String,
    pub snapshot_path: String,
    pub compressed_size: i64,
    pub original_size: i64,
    pub file_hash: Option<String>,
    pub created_at: String,
    pub scan_batch_id: Option<i64>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct SnapshotFileGroup {
    pub original_path: String,
    pub original_filename: String,
    pub snapshot_count: i64,
    pub total_size: i64,
    pub latest_snapshot: String,
    pub oldest_snapshot: String,
    pub file_exists: bool,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct RecycleBinEntry {
    pub id: i64,
    pub original_path: String,
    pub filename: String,
    pub original_size: i64,
    pub deleted_at: String,
    pub is_recoverable: bool,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct CloudFolder {
    pub id: i64,
    pub path: String,
    pub provider: String,
    pub display_name: Option<String>,
    pub is_active: bool,
    pub detected_at: String,
}

#[derive(Debug, Clone, serde::Serialize, Default)]
pub struct RecoveryStats {
    pub recycle_bin_count: i64,
    pub snapshot_count: i64,
    pub total_snapshot_size: i64,
    pub cloud_folders_count: i64,
}

// --- Phase 1 feature structs ---

#[derive(Debug, Clone, serde::Serialize)]
pub struct IgnorePattern {
    pub id: i64,
    pub folder_id: i64,
    pub pattern: String,
    pub pattern_type: String,
    pub created_at: String,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct SnapshotTag {
    pub id: i64,
    pub snapshot_id: i64,
    pub name: String,
    pub description: Option<String>,
    pub color: String,
    pub created_at: String,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct WorkspaceProfile {
    pub id: i64,
    pub name: String,
    pub is_active: bool,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct HeatmapEntry {
    pub date: String,
    pub count: i64,
}

// --- Phase 2 structs ---

#[derive(Debug, Clone, serde::Serialize)]
pub struct NotificationProfile {
    pub id: i64,
    pub name: String,
    pub quiet_hours_start: i64,
    pub quiet_hours_end: i64,
    pub notify_new: bool,
    pub notify_modified: bool,
    pub notify_deleted: bool,
    pub notify_moved: bool,
    pub enabled: bool,
    pub created_at: String,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct WebhookEndpoint {
    pub id: i64,
    pub name: String,
    pub url: String,
    pub events: String,
    #[serde(skip_serializing)]
    pub secret: Option<String>,
    pub enabled: bool,
    pub last_triggered: Option<String>,
    pub last_status: Option<i64>,
    pub created_at: String,
}

/// Version of WebhookEndpoint sent to frontend with has_secret indicator
#[derive(Debug, Clone, serde::Serialize)]
pub struct WebhookEndpointSafe {
    pub id: i64,
    pub name: String,
    pub url: String,
    pub events: String,
    pub has_secret: bool,
    pub enabled: bool,
    pub last_triggered: Option<String>,
    pub last_status: Option<i64>,
    pub created_at: String,
}

impl From<WebhookEndpoint> for WebhookEndpointSafe {
    fn from(ep: WebhookEndpoint) -> Self {
        WebhookEndpointSafe {
            id: ep.id,
            name: ep.name,
            url: ep.url,
            events: ep.events,
            has_secret: ep.secret.is_some(),
            enabled: ep.enabled,
            last_triggered: ep.last_triggered,
            last_status: ep.last_status,
            created_at: ep.created_at,
        }
    }
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct BlameLine {
    pub line_number: usize,
    pub content: String,
    pub change_type: String,
    pub scan_batch_id: Option<i64>,
    pub detected_at: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct ChangelogEntry {
    pub date: String,
    pub batch_id: i64,
    pub folders_scanned: String,
    pub total_files: i64,
    pub new_files: i64,
    pub modified_files: i64,
    pub deleted_files: i64,
    pub moved_files: i64,
    pub changes: Vec<ChangeRecord>,
}

// --- Phase 3 structs ---

#[derive(Debug, Clone, serde::Serialize)]
pub struct ExtensionStat {
    pub extension: String,
    pub count: i64,
    pub total_size: i64,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct DailyTrend {
    pub date: String,
    pub new_count: i64,
    pub modified_count: i64,
    pub deleted_count: i64,
    pub moved_count: i64,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct AdvancedSearchResult {
    pub records: Vec<ChangeRecord>,
    pub total_count: i64,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct ExportData {
    pub generated_at: String,
    pub summary: ChangeStats,
    pub batches: Vec<ChangelogEntry>,
    pub extension_stats: Vec<ExtensionStat>,
    pub trends: Vec<DailyTrend>,
    pub snapshots: Vec<FileSnapshotRecord>,
    pub duplicate_groups: Vec<DuplicateGroupRecord>,
    pub monitored_folders: Vec<MonitoredFolder>,
}

// --- Recovery feature database methods ---

impl Database {
    pub fn insert_file_snapshot(
        &self,
        original_path: &str,
        original_filename: &str,
        snapshot_path: &str,
        compressed_size: i64,
        original_size: i64,
        file_hash: Option<&str>,
    ) -> Result<i64> {
        let conn = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        conn.execute(
            "INSERT INTO file_snapshots (original_path, original_filename, snapshot_path, compressed_size, original_size, file_hash)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![original_path, original_filename, snapshot_path, compressed_size, original_size, file_hash],
        )?;
        Ok(conn.last_insert_rowid())
    }

    pub fn get_snapshots_for_file(&self, original_path: &str) -> Result<Vec<FileSnapshotRecord>> {
        let conn = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        let mut stmt = conn.prepare(
            "SELECT id, original_path, original_filename, snapshot_path, compressed_size, original_size, file_hash, created_at, scan_batch_id
             FROM file_snapshots WHERE original_path = ?1 ORDER BY created_at DESC",
        )?;
        let rows = stmt.query_map(params![original_path], |row| {
            Ok(FileSnapshotRecord {
                id: row.get(0)?,
                original_path: row.get(1)?,
                original_filename: row.get(2)?,
                snapshot_path: row.get(3)?,
                compressed_size: row.get(4)?,
                original_size: row.get(5)?,
                file_hash: row.get(6)?,
                created_at: row.get(7)?,
                scan_batch_id: row.get(8)?,
            })
        })?;
        rows.collect::<Result<Vec<_>>>()
    }

    pub fn get_all_file_snapshots(&self, limit: i64) -> Result<Vec<FileSnapshotRecord>> {
        let conn = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        let mut stmt = conn.prepare(
            "SELECT id, original_path, original_filename, snapshot_path, compressed_size, original_size, file_hash, created_at, scan_batch_id
             FROM file_snapshots ORDER BY created_at DESC LIMIT ?1",
        )?;
        let rows = stmt.query_map(params![limit], |row| {
            Ok(FileSnapshotRecord {
                id: row.get(0)?,
                original_path: row.get(1)?,
                original_filename: row.get(2)?,
                snapshot_path: row.get(3)?,
                compressed_size: row.get(4)?,
                original_size: row.get(5)?,
                file_hash: row.get(6)?,
                created_at: row.get(7)?,
                scan_batch_id: row.get(8)?,
            })
        })?;
        rows.collect::<Result<Vec<_>>>()
    }

    pub fn get_snapshots_grouped_by_file(&self) -> Result<Vec<SnapshotFileGroup>> {
        let conn = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        let mut stmt = conn.prepare(
            "SELECT
                original_path,
                original_filename,
                COUNT(*) as snapshot_count,
                COALESCE(SUM(compressed_size), 0) as total_size,
                MAX(created_at) as latest,
                MIN(created_at) as oldest
             FROM file_snapshots
             GROUP BY original_path
             ORDER BY MAX(created_at) DESC",
        )?;
        let rows = stmt.query_map([], |row| {
            let path: String = row.get(0)?;
            let file_exists = std::path::Path::new(&path).exists();
            Ok(SnapshotFileGroup {
                original_path: path,
                original_filename: row.get(1)?,
                snapshot_count: row.get(2)?,
                total_size: row.get(3)?,
                latest_snapshot: row.get(4)?,
                oldest_snapshot: row.get(5)?,
                file_exists,
            })
        })?;
        rows.collect::<Result<Vec<_>>>()
    }

    pub fn get_file_snapshot_stats(&self) -> Result<(i64, i64)> {
        let conn = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        let mut stmt = conn.prepare(
            "SELECT COUNT(*), COALESCE(SUM(compressed_size), 0) FROM file_snapshots",
        )?;
        let mut rows = stmt.query_map([], |row| {
            Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?))
        })?;
        match rows.next() {
            Some(row) => Ok(row?),
            None => Ok((0, 0)),
        }
    }

    pub fn delete_file_snapshot(&self, id: i64) -> Result<()> {
        let conn = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        conn.execute("DELETE FROM file_snapshots WHERE id = ?1", params![id])?;
        Ok(())
    }

    pub fn cleanup_old_file_snapshots(&self, keep_days: i64) -> Result<i64> {
        let conn = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        let deleted = conn.execute(
            "DELETE FROM file_snapshots WHERE created_at < DATETIME('now', '-' || ?1 || ' days')",
            params![keep_days],
        )?;
        Ok(deleted as i64)
    }

    /// Get a single snapshot by ID (targeted query, not full table scan)
    pub fn get_file_snapshot_by_id(&self, id: i64) -> Result<Option<FileSnapshotRecord>> {
        let conn = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        let mut stmt = conn.prepare(
            "SELECT id, original_path, original_filename, snapshot_path, compressed_size, original_size, file_hash, created_at, scan_batch_id
             FROM file_snapshots WHERE id = ?1",
        )?;
        let mut rows = stmt.query_map(params![id], |row| {
            Ok(FileSnapshotRecord {
                id: row.get(0)?,
                original_path: row.get(1)?,
                original_filename: row.get(2)?,
                snapshot_path: row.get(3)?,
                compressed_size: row.get(4)?,
                original_size: row.get(5)?,
                file_hash: row.get(6)?,
                created_at: row.get(7)?,
                scan_batch_id: row.get(8)?,
            })
        })?;
        match rows.next() {
            Some(row) => Ok(Some(row?)),
            None => Ok(None),
        }
    }

    /// Get decompressed content of a snapshot file
    pub fn get_snapshot_content(&self, snapshot_id: i64) -> Result<Option<String>> {
        let snapshot = self.get_file_snapshot_by_id(snapshot_id)?;
        match snapshot {
            Some(s) => {
                let path = std::path::Path::new(&s.snapshot_path);
                if !path.exists() {
                    return Ok(None);
                }
                let compressed = std::fs::read(path).map_err(|e| rusqlite::Error::InvalidParameterName(format!("Failed to read snapshot: {}", e)))?;
                let mut decoder = zstd::Decoder::new(&compressed[..]).map_err(|e| rusqlite::Error::InvalidParameterName(format!("Failed to create decoder: {}", e)))?;
                // Cap decompressed size at 50MB to prevent zip-bomb OOM
                const MAX_DECOMPRESSED: usize = 50 * 1024 * 1024;
                let mut content = String::new();
                let mut buf = [0u8; 8192];
                loop {
                    let n = std::io::Read::read(&mut decoder, &mut buf)
                        .map_err(|e| rusqlite::Error::InvalidParameterName(format!("Failed to decompress: {}", e)))?;
                    if n == 0 { break; }
                    if content.len() + n > MAX_DECOMPRESSED {
                        return Err(rusqlite::Error::InvalidParameterName(
                            "Snapshot content exceeds 50MB decompression limit".to_string()
                        ));
                    }
                    content.push_str(std::str::from_utf8(&buf[..n])
                        .map_err(|e| rusqlite::Error::InvalidParameterName(format!("Invalid UTF-8: {}", e)))?);
                }
                Ok(Some(content))
            }
            None => Ok(None),
        }
    }

    /// Get current file content as string (with 10MB size limit to prevent OOM)
    pub fn get_file_content(&self, path: &str) -> Result<Option<String>> {
        let file_path = std::path::Path::new(path);
        if !file_path.exists() {
            return Ok(None);
        }
        // Guard against reading extremely large files
        match std::fs::metadata(file_path) {
            Ok(meta) => {
                if meta.len() > 10 * 1024 * 1024 {
                    return Err(rusqlite::Error::InvalidParameterName(
                        "File too large to display (max 10MB)".to_string(),
                    ));
                }
            }
            Err(_) => return Ok(None),
        }
        match std::fs::read_to_string(file_path) {
            Ok(content) => Ok(Some(content)),
            Err(_) => Ok(None), // Binary file or read error
        }
    }

    /// Get snapshot paths older than N days (for physical file cleanup before DB delete)
    pub fn get_old_snapshot_paths(&self, keep_days: i64) -> Result<Vec<String>> {
        let conn = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        let mut stmt = conn.prepare(
            "SELECT snapshot_path FROM file_snapshots WHERE created_at < DATETIME('now', '-' || ?1 || ' days')",
        )?;
        let rows = stmt.query_map(params![keep_days], |row| {
            row.get::<_, String>(0)
        })?;
        rows.collect::<Result<Vec<_>>>()
    }

    // Recycle bin methods

    pub fn insert_recycle_bin_entry(
        &self,
        original_path: &str,
        filename: &str,
        original_size: i64,
        deleted_at: &str,
    ) -> Result<i64> {
        let conn = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        // Use ON CONFLICT to preserve is_recoverable state — don't reset to 1
        conn.execute(
            "INSERT INTO recycle_bin_entries (original_path, filename, original_size, deleted_at, is_recoverable, last_checked)
             VALUES (?1, ?2, ?3, ?4, 1, CURRENT_TIMESTAMP)
             ON CONFLICT(original_path) DO UPDATE SET
                filename = excluded.filename,
                original_size = excluded.original_size,
                deleted_at = excluded.deleted_at,
                last_checked = CURRENT_TIMESTAMP
             WHERE is_recoverable = 1",
            params![original_path, filename, original_size, deleted_at],
        )?;
        Ok(conn.last_insert_rowid())
    }

    pub fn get_recoverable_files(&self) -> Result<Vec<RecycleBinEntry>> {
        let conn = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        let mut stmt = conn.prepare(
            "SELECT id, original_path, filename, original_size, deleted_at, is_recoverable
             FROM recycle_bin_entries WHERE is_recoverable = 1 ORDER BY deleted_at DESC",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(RecycleBinEntry {
                id: row.get(0)?,
                original_path: row.get(1)?,
                filename: row.get(2)?,
                original_size: row.get(3)?,
                deleted_at: row.get(4)?,
                is_recoverable: row.get(5)?,
            })
        })?;
        rows.collect::<Result<Vec<_>>>()
    }

    pub fn mark_recycle_bin_recovered(&self, id: i64) -> Result<()> {
        let conn = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        conn.execute(
            "UPDATE recycle_bin_entries SET is_recoverable = 0 WHERE id = ?1",
            params![id],
        )?;
        Ok(())
    }

    pub fn clear_old_recycle_bin_entries(&self) -> Result<()> {
        let conn = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        conn.execute(
            "DELETE FROM recycle_bin_entries WHERE last_checked < DATETIME('now', '-7 days')",
            [],
        )?;
        Ok(())
    }

    // Cloud folder methods

    pub fn upsert_cloud_folder(&self, path: &str, provider: &str, display_name: Option<&str>) -> Result<i64> {
        let conn = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        conn.execute(
            "INSERT OR REPLACE INTO cloud_folders (path, provider, display_name, is_active, detected_at)
             VALUES (?1, ?2, ?3, 1, CURRENT_TIMESTAMP)",
            params![path, provider, display_name],
        )?;
        Ok(conn.last_insert_rowid())
    }

    pub fn get_cloud_folders(&self) -> Result<Vec<CloudFolder>> {
        let conn = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        let mut stmt = conn.prepare(
            "SELECT id, path, provider, display_name, is_active, detected_at FROM cloud_folders WHERE is_active = 1",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(CloudFolder {
                id: row.get(0)?,
                path: row.get(1)?,
                provider: row.get(2)?,
                display_name: row.get(3)?,
                is_active: row.get(4)?,
                detected_at: row.get(5)?,
            })
        })?;
        rows.collect::<Result<Vec<_>>>()
    }

    pub fn is_path_in_cloud_folder(&self, path: &str) -> Result<Option<String>> {
        let folders = self.get_cloud_folders()?;
        for folder in folders {
            if path.starts_with(&folder.path) || path.to_lowercase().starts_with(&folder.path.to_lowercase()) {
                return Ok(Some(folder.provider));
            }
        }
        Ok(None)
    }

    // Recovery audit log

    pub fn log_recovery_action(&self, action: &str, details: Option<&str>, success: bool, error_message: Option<&str>) -> Result<()> {
        let conn = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        conn.execute(
            "INSERT INTO recovery_audit_log (action, details, success, error_message) VALUES (?1, ?2, ?3, ?4)",
            params![action, details, success, error_message],
        )?;
        Ok(())
    }

    /// Cleanup audit log entries older than N days (default 90)
    pub fn cleanup_old_audit_logs(&self, keep_days: i64) -> Result<i64> {
        let conn = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        let deleted = conn.execute(
            "DELETE FROM recovery_audit_log WHERE performed_at < DATETIME('now', '-' || ?1 || ' days')",
            params![keep_days],
        )?;
        Ok(deleted as i64)
    }

    // --- Ignore Pattern methods ---

    pub fn add_ignore_pattern(&self, folder_id: i64, pattern: &str, pattern_type: &str) -> Result<i64> {
        let conn = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        // M10: Reject duplicate patterns for the same folder
        let exists: bool = conn.query_row(
            "SELECT COUNT(*) > 0 FROM ignore_patterns WHERE folder_id = ?1 AND pattern = ?2 AND pattern_type = ?3",
            params![folder_id, pattern, pattern_type],
            |row| row.get(0),
        ).unwrap_or(false);
        if exists {
            return Err(rusqlite::Error::InvalidParameterName(
                format!("Pattern '{}' of type '{}' already exists for this folder", pattern, pattern_type)
            ));
        }
        conn.execute(
            "INSERT INTO ignore_patterns (folder_id, pattern, pattern_type) VALUES (?1, ?2, ?3)",
            params![folder_id, pattern, pattern_type],
        )?;
        Ok(conn.last_insert_rowid())
    }

    pub fn remove_ignore_pattern(&self, id: i64) -> Result<()> {
        let conn = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        conn.execute("DELETE FROM ignore_patterns WHERE id = ?1", params![id])?;
        Ok(())
    }

    pub fn get_ignore_patterns_for_folder(&self, folder_id: i64) -> Result<Vec<IgnorePattern>> {
        let conn = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        let mut stmt = conn.prepare(
            "SELECT id, folder_id, pattern, pattern_type, created_at FROM ignore_patterns WHERE folder_id = ?1 ORDER BY created_at DESC",
        )?;
        let rows = stmt.query_map(params![folder_id], |row| {
            Ok(IgnorePattern {
                id: row.get(0)?,
                folder_id: row.get(1)?,
                pattern: row.get(2)?,
                pattern_type: row.get(3)?,
                created_at: row.get(4)?,
            })
        })?;
        rows.collect::<Result<Vec<_>>>()
    }

    pub fn get_all_ignore_patterns(&self) -> Result<Vec<IgnorePattern>> {
        let conn = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        let mut stmt = conn.prepare(
            "SELECT id, folder_id, pattern, pattern_type, created_at FROM ignore_patterns ORDER BY folder_id, created_at",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(IgnorePattern {
                id: row.get(0)?,
                folder_id: row.get(1)?,
                pattern: row.get(2)?,
                pattern_type: row.get(3)?,
                created_at: row.get(4)?,
            })
        })?;
        rows.collect::<Result<Vec<_>>>()
    }

    /// Check if a file path matches any ignore pattern for its parent folder
    pub fn should_ignore_file(&self, file_path: &str, folder_id: i64) -> bool {
        let patterns = self.get_ignore_patterns_for_folder(folder_id).unwrap_or_default();
        let path_lower = file_path.replace('\\', "/").to_lowercase();
        for p in &patterns {
            match p.pattern_type.as_str() {
                "glob" => {
                    // Simple glob matching: convert glob to a check
                    let pat = p.pattern.replace('\\', "/");
                    if glob_match(&pat, &path_lower) {
                        return true;
                    }
                }
                "regex" => {
                    if let Ok(re) = regex_simple(&p.pattern) {
                        if re.is_match(&path_lower) {
                            return true;
                        }
                    }
                }
                "contains" => {
                    if path_lower.contains(&p.pattern.to_lowercase()) {
                        return true;
                    }
                }
                _ => {}
            }
        }
        false
    }

    // --- Snapshot Tag methods ---

    pub fn add_snapshot_tag(&self, snapshot_id: i64, name: &str, description: Option<&str>, color: Option<&str>) -> Result<i64> {
        let conn = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        conn.execute(
            "INSERT OR IGNORE INTO snapshot_tags (snapshot_id, name, description, color) VALUES (?1, ?2, ?3, ?4)",
            params![snapshot_id, name, description.unwrap_or(""), color.unwrap_or("#6366f1")],
        )?;
        Ok(conn.last_insert_rowid())
    }

    pub fn remove_snapshot_tag(&self, id: i64) -> Result<()> {
        let conn = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        conn.execute("DELETE FROM snapshot_tags WHERE id = ?1", params![id])?;
        Ok(())
    }

    pub fn get_tags_for_snapshot(&self, snapshot_id: i64) -> Result<Vec<SnapshotTag>> {
        let conn = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        let mut stmt = conn.prepare(
            "SELECT id, snapshot_id, name, description, color, created_at FROM snapshot_tags WHERE snapshot_id = ?1 ORDER BY created_at DESC",
        )?;
        let rows = stmt.query_map(params![snapshot_id], |row| {
            Ok(SnapshotTag {
                id: row.get(0)?,
                snapshot_id: row.get(1)?,
                name: row.get(2)?,
                description: row.get(3)?,
                color: row.get(4)?,
                created_at: row.get(5)?,
            })
        })?;
        rows.collect::<Result<Vec<_>>>()
    }

    pub fn get_all_tags(&self) -> Result<Vec<SnapshotTag>> {
        let conn = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        let mut stmt = conn.prepare(
            "SELECT id, snapshot_id, name, description, color, created_at FROM snapshot_tags ORDER BY created_at DESC",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(SnapshotTag {
                id: row.get(0)?,
                snapshot_id: row.get(1)?,
                name: row.get(2)?,
                description: row.get(3)?,
                color: row.get(4)?,
                created_at: row.get(5)?,
            })
        })?;
        rows.collect::<Result<Vec<_>>>()
    }

    pub fn get_snapshots_with_tag(&self, tag_name: &str) -> Result<Vec<FileSnapshotRecord>> {
        let conn = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        let mut stmt = conn.prepare(
            "SELECT fs.id, fs.original_path, fs.original_filename, fs.snapshot_path, fs.compressed_size, fs.original_size, fs.file_hash, fs.created_at, fs.scan_batch_id
             FROM file_snapshots fs
             JOIN snapshot_tags st ON fs.id = st.snapshot_id
             WHERE st.name = ?1
             ORDER BY fs.created_at DESC",
        )?;
        let rows = stmt.query_map(params![tag_name], |row| {
            Ok(FileSnapshotRecord {
                id: row.get(0)?,
                original_path: row.get(1)?,
                original_filename: row.get(2)?,
                snapshot_path: row.get(3)?,
                compressed_size: row.get(4)?,
                original_size: row.get(5)?,
                file_hash: row.get(6)?,
                created_at: row.get(7)?,
                scan_batch_id: row.get(8)?,
            })
        })?;
        rows.collect::<Result<Vec<_>>>()
    }

    /// Compare two snapshots and return their diff content
    pub fn compare_snapshots(&self, snapshot_a_id: i64, snapshot_b_id: i64) -> Result<Option<(String, String)>> {
        let a = self.get_snapshot_content(snapshot_a_id)?;
        let b = self.get_snapshot_content(snapshot_b_id)?;
        Ok(match (a, b) {
            (Some(a_content), Some(b_content)) => Some((a_content, b_content)),
            _ => None,
        })
    }

    // --- Workspace Profile methods ---

    pub fn create_profile(&self, name: &str) -> Result<i64> {
        let conn = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        conn.execute(
            "INSERT INTO workspace_profiles (name) VALUES (?1)",
            params![name],
        )?;
        Ok(conn.last_insert_rowid())
    }

    pub fn delete_profile(&self, id: i64) -> Result<()> {
        let conn = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        conn.execute("DELETE FROM workspace_profiles WHERE id = ?1", params![id])?;
        Ok(())
    }

    pub fn get_all_profiles(&self) -> Result<Vec<WorkspaceProfile>> {
        let conn = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        let mut stmt = conn.prepare(
            "SELECT id, name, is_active, created_at, updated_at FROM workspace_profiles ORDER BY is_active DESC, name ASC",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(WorkspaceProfile {
                id: row.get(0)?,
                name: row.get(1)?,
                is_active: row.get(2)?,
                created_at: row.get(3)?,
                updated_at: row.get(4)?,
            })
        })?;
        rows.collect::<Result<Vec<_>>>()
    }

    pub fn activate_profile(&self, profile_id: i64) -> Result<()> {
        let conn = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        // Deactivate all profiles
        conn.execute("UPDATE workspace_profiles SET is_active = 0", [])?;
        // Activate the selected one
        conn.execute("UPDATE workspace_profiles SET is_active = 1, updated_at = CURRENT_TIMESTAMP WHERE id = ?1", params![profile_id])?;
        // Enable only folders in this profile
        conn.execute("UPDATE monitored_folders SET enabled = 0", [])?;
        conn.execute(
            "UPDATE monitored_folders SET enabled = 1 WHERE id IN (SELECT folder_id FROM profile_folders WHERE profile_id = ?1)",
            params![profile_id],
        )?;
        Ok(())
    }

    pub fn save_current_folders_to_profile(&self, profile_id: i64) -> Result<()> {
        let conn = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        // Clear existing profile folders
        conn.execute("DELETE FROM profile_folders WHERE profile_id = ?1", params![profile_id])?;
        // Copy currently enabled folders to the profile
        conn.execute(
            "INSERT INTO profile_folders (profile_id, folder_id)
             SELECT ?1, id FROM monitored_folders WHERE enabled = 1",
            params![profile_id],
        )?;
        conn.execute(
            "UPDATE workspace_profiles SET updated_at = CURRENT_TIMESTAMP WHERE id = ?1",
            params![profile_id],
        )?;
        Ok(())
    }

    pub fn get_folders_for_profile(&self, profile_id: i64) -> Result<Vec<MonitoredFolder>> {
        let conn = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        let mut stmt = conn.prepare(
            "SELECT mf.id, mf.path, mf.enabled, mf.added_at
             FROM monitored_folders mf
             JOIN profile_folders pf ON mf.id = pf.folder_id
             WHERE pf.profile_id = ?1
             ORDER BY mf.added_at",
        )?;
        let rows = stmt.query_map(params![profile_id], |row| {
            Ok(MonitoredFolder {
                id: row.get(0)?,
                path: row.get(1)?,
                enabled: row.get(2)?,
                added_at: row.get(3)?,
            })
        })?;
        rows.collect::<Result<Vec<_>>>()
    }

    // --- File History methods ---

    /// Get all changes for a specific file path (for timeline view)
    pub fn get_file_history(&self, file_path: &str) -> Result<Vec<ChangeRecord>> {
        let conn = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        let mut stmt = conn.prepare(
            "SELECT c.id, c.file_id, f.path, f.filename, c.change_type, c.detected_at, c.previous_path, c.new_path
             FROM changes c
             JOIN files f ON c.file_id = f.id
             WHERE f.path = ?1 OR c.previous_path = ?1 OR c.new_path = ?1
             ORDER BY c.detected_at DESC",
        )?;
        let rows = stmt.query_map(params![file_path], |row| {
            Ok(ChangeRecord {
                id: row.get(0)?,
                file_id: row.get(1)?,
                file_path: row.get(2)?,
                filename: row.get(3)?,
                change_type: row.get(4)?,
                detected_at: row.get(5)?,
                previous_path: row.get(6)?,
                new_path: row.get(7)?,
            })
        })?;
        rows.collect::<Result<Vec<_>>>()
    }

    /// Get recent changes with search query (for searchable event history)
    pub fn search_changes(&self, query: &str, limit: i64) -> Result<Vec<ChangeRecord>> {
        let conn = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        // Escape LIKE wildcards in user query to treat them as literal characters
        let escaped_query = query.to_lowercase().replace('\\', "\\\\").replace('%', "\\%").replace('_', "\\_");
        let search_pattern = format!("%{}%", escaped_query);
        let mut stmt = conn.prepare(
            "SELECT c.id, c.file_id, f.path, f.filename, c.change_type, c.detected_at, c.previous_path, c.new_path
             FROM changes c
             JOIN files f ON c.file_id = f.id
             WHERE LOWER(f.path) LIKE ?1 OR LOWER(f.filename) LIKE ?1 OR LOWER(c.change_type) LIKE ?1
             ORDER BY c.detected_at DESC
             LIMIT ?2",
        )?;
        let rows = stmt.query_map(params![search_pattern, limit], |row| {
            Ok(ChangeRecord {
                id: row.get(0)?,
                file_id: row.get(1)?,
                file_path: row.get(2)?,
                filename: row.get(3)?,
                change_type: row.get(4)?,
                detected_at: row.get(5)?,
                previous_path: row.get(6)?,
                new_path: row.get(7)?,
            })
        })?;
        rows.collect::<Result<Vec<_>>>()
    }

    /// Get activity heatmap data (changes per day for the last N days)
    pub fn get_activity_heatmap(&self, days: i64) -> Result<Vec<HeatmapEntry>> {
        let conn = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        let mut stmt = conn.prepare(
            "SELECT DATE(detected_at) as date, COUNT(*) as count
             FROM changes
             WHERE detected_at >= DATE('now', '-' || ?1 || ' days')
             GROUP BY DATE(detected_at)
             ORDER BY date ASC",
        )?;
        let rows = stmt.query_map(params![days], |row| {
            Ok(HeatmapEntry {
                date: row.get(0)?,
                count: row.get(1)?,
            })
        })?;
        rows.collect::<Result<Vec<_>>>()
    }

    pub fn get_recovery_stats(&self) -> Result<RecoveryStats> {
        let conn = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        let mut stmt = conn.prepare(
            "SELECT
                (SELECT COUNT(*) FROM recycle_bin_entries WHERE is_recoverable = 1),
                (SELECT COUNT(*) FROM file_snapshots),
                (SELECT COALESCE(SUM(compressed_size), 0) FROM file_snapshots),
                (SELECT COUNT(*) FROM cloud_folders WHERE is_active = 1)",
        )?;
        let mut rows = stmt.query_map([], |row| {
            Ok(RecoveryStats {
                recycle_bin_count: row.get(0)?,
                snapshot_count: row.get(1)?,
                total_snapshot_size: row.get(2)?,
                cloud_folders_count: row.get(3)?,
            })
        })?;
        match rows.next() {
            Some(row) => Ok(row?),
            None => Ok(RecoveryStats::default()),
        }
    }

    // ==================== PHASE 2: NOTIFICATION PROFILES ====================

    pub fn create_notification_profile(
        &self,
        name: &str,
        quiet_hours_start: i64,
        quiet_hours_end: i64,
    ) -> Result<i64> {
        let conn = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        conn.execute(
            "INSERT INTO notification_profiles (name, quiet_hours_start, quiet_hours_end) VALUES (?1, ?2, ?3)",
            params![name, quiet_hours_start, quiet_hours_end],
        )?;
        Ok(conn.last_insert_rowid())
    }

    pub fn delete_notification_profile(&self, id: i64) -> Result<()> {
        let conn = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        conn.execute("DELETE FROM notification_profiles WHERE id = ?1", params![id])?;
        Ok(())
    }

    pub fn get_all_notification_profiles(&self) -> Result<Vec<NotificationProfile>> {
        let conn = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        let mut stmt = conn.prepare(
            "SELECT id, name, quiet_hours_start, quiet_hours_end, notify_new, notify_modified, notify_deleted, notify_moved, enabled, created_at
             FROM notification_profiles ORDER BY name",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(NotificationProfile {
                id: row.get(0)?,
                name: row.get(1)?,
                quiet_hours_start: row.get(2)?,
                quiet_hours_end: row.get(3)?,
                notify_new: row.get(4)?,
                notify_modified: row.get(5)?,
                notify_deleted: row.get(6)?,
                notify_moved: row.get(7)?,
                enabled: row.get(8)?,
                created_at: row.get(9)?,
            })
        })?;
        rows.collect::<Result<Vec<_>>>()
    }

    pub fn update_notification_profile(
        &self,
        id: i64,
        quiet_hours_start: Option<i64>,
        quiet_hours_end: Option<i64>,
        notify_new: Option<bool>,
        notify_modified: Option<bool>,
        notify_deleted: Option<bool>,
        notify_moved: Option<bool>,
        enabled: Option<bool>,
    ) -> Result<()> {
        let conn = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(v) = quiet_hours_start {
            conn.execute("UPDATE notification_profiles SET quiet_hours_start = ?1 WHERE id = ?2", params![v, id])?;
        }
        if let Some(v) = quiet_hours_end {
            conn.execute("UPDATE notification_profiles SET quiet_hours_end = ?1 WHERE id = ?2", params![v, id])?;
        }
        if let Some(v) = notify_new {
            conn.execute("UPDATE notification_profiles SET notify_new = ?1 WHERE id = ?2", params![v, id])?;
        }
        if let Some(v) = notify_modified {
            conn.execute("UPDATE notification_profiles SET notify_modified = ?1 WHERE id = ?2", params![v, id])?;
        }
        if let Some(v) = notify_deleted {
            conn.execute("UPDATE notification_profiles SET notify_deleted = ?1 WHERE id = ?2", params![v, id])?;
        }
        if let Some(v) = notify_moved {
            conn.execute("UPDATE notification_profiles SET notify_moved = ?1 WHERE id = ?2", params![v, id])?;
        }
        if let Some(v) = enabled {
            conn.execute("UPDATE notification_profiles SET enabled = ?1 WHERE id = ?2", params![v, id])?;
        }
        Ok(())
    }

    pub fn set_notification_profile_folders(&self, profile_id: i64, folder_ids: &[i64]) -> Result<()> {
        let conn = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        conn.execute("DELETE FROM notification_profile_folders WHERE profile_id = ?1", params![profile_id])?;
        for fid in folder_ids {
            conn.execute(
                "INSERT OR IGNORE INTO notification_profile_folders (profile_id, folder_id) VALUES (?1, ?2)",
                params![profile_id, fid],
            )?;
        }
        Ok(())
    }

    pub fn get_folders_for_notification_profile(&self, profile_id: i64) -> Result<Vec<MonitoredFolder>> {
        let conn = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        let mut stmt = conn.prepare(
            "SELECT mf.id, mf.path, mf.enabled, mf.added_at
             FROM monitored_folders mf
             JOIN notification_profile_folders npf ON mf.id = npf.folder_id
             WHERE npf.profile_id = ?1",
        )?;
        let rows = stmt.query_map(params![profile_id], |row| {
            Ok(MonitoredFolder {
                id: row.get(0)?,
                path: row.get(1)?,
                enabled: row.get(2)?,
                added_at: row.get(3)?,
            })
        })?;
        rows.collect::<Result<Vec<_>>>()
    }

    /// Check if a notification should be sent for this change type right now
    pub fn should_send_notification(&self, change_type: &str) -> bool {
        let profiles = match self.get_all_notification_profiles() {
            Ok(p) => p,
            Err(_) => return true, // default to sending
        };
        let enabled_profiles: Vec<_> = profiles.into_iter().filter(|p| p.enabled).collect();
        if enabled_profiles.is_empty() {
            return true; // no profiles = default allow
        }
        for profile in &enabled_profiles {
            let type_allowed = match change_type {
                "NEW" => profile.notify_new,
                "MODIFIED" => profile.notify_modified,
                "DELETED" => profile.notify_deleted,
                "MOVED" => profile.notify_moved,
                _ => true,
            };
            if !type_allowed {
                continue;
            }
            // Check quiet hours
            let now: i64 = chrono::Local::now().format("%H").to_string().parse().unwrap_or(0);
            let in_quiet = if profile.quiet_hours_start <= profile.quiet_hours_end {
                now >= profile.quiet_hours_start && now < profile.quiet_hours_end
            } else {
                // Wraps midnight, e.g. 22..6
                now >= profile.quiet_hours_start || now < profile.quiet_hours_end
            };
            if !in_quiet {
                return true; // at least one profile allows it
            }
        }
        false
    }

    // ==================== PHASE 2: WEBHOOKS ====================

    pub fn create_webhook_endpoint(&self, name: &str, url: &str, events: &str, secret: Option<&str>) -> Result<i64> {
        let conn = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        conn.execute(
            "INSERT INTO webhook_endpoints (name, url, events, secret) VALUES (?1, ?2, ?3, ?4)",
            params![name, url, events, secret],
        )?;
        Ok(conn.last_insert_rowid())
    }

    pub fn delete_webhook_endpoint(&self, id: i64) -> Result<()> {
        let conn = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        conn.execute("DELETE FROM webhook_endpoints WHERE id = ?1", params![id])?;
        Ok(())
    }

    pub fn get_all_webhook_endpoints(&self) -> Result<Vec<WebhookEndpoint>> {
        let conn = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        let mut stmt = conn.prepare(
            "SELECT id, name, url, events, secret, enabled, last_triggered, last_status, created_at
             FROM webhook_endpoints ORDER BY name",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(WebhookEndpoint {
                id: row.get(0)?,
                name: row.get(1)?,
                url: row.get(2)?,
                events: row.get(3)?,
                secret: row.get(4)?,
                enabled: row.get(5)?,
                last_triggered: row.get(6)?,
                last_status: row.get(7)?,
                created_at: row.get(8)?,
            })
        })?;
        rows.collect::<Result<Vec<_>>>()
    }

    pub fn toggle_webhook_endpoint(&self, id: i64, enabled: bool) -> Result<()> {
        let conn = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        conn.execute("UPDATE webhook_endpoints SET enabled = ?1 WHERE id = ?2", params![enabled, id])?;
        Ok(())
    }

    pub fn update_webhook_trigger(&self, id: i64, status: i64) -> Result<()> {
        let conn = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        conn.execute(
            "UPDATE webhook_endpoints SET last_triggered = CURRENT_TIMESTAMP, last_status = ?1 WHERE id = ?2",
            params![status, id],
        )?;
        Ok(())
    }

    /// Get webhook endpoints that match the given change type
    /// Uses case-insensitive comparison so "new", "NEW", "New" all match.
    pub fn get_active_webhooks_for_event(&self, change_type: &str) -> Result<Vec<WebhookEndpoint>> {
        let all = self.get_all_webhook_endpoints()?;
        let ct_upper = change_type.to_uppercase();
        Ok(all.into_iter().filter(|wh| {
            if !wh.enabled { return false; }
            let events_upper = wh.events.to_uppercase();
            if events_upper == "ALL" { return true; }
            events_upper.split(',').any(|e| e.trim() == ct_upper || e.trim() == "ALL")
        }).collect())
    }

    /// One-time migration: encrypt any plaintext webhook secrets.
    /// Called once during setup after CryptoManager is initialized.
    /// `is_encrypted_fn` checks if a value is already encrypted.
    /// `encrypt_fn` encrypts a plaintext value.
    pub fn migrate_plaintext_secrets(
        &self,
        is_encrypted_fn: &dyn Fn(&str) -> bool,
        encrypt_fn: &dyn Fn(&str) -> Result<String, String>,
    ) -> Result<u64, String> {
        let conn = self.conn.lock().map_err(|e| e.to_string());
        let conn = match conn {
            Ok(c) => c,
            Err(e) => return Err(format!("DB lock poisoned: {}", e)),
        };

        // Check if migration already completed
        let migration_done: bool = conn
            .prepare("SELECT value FROM settings WHERE key = 'webhook_secret_migration_done'")
            .and_then(|mut stmt| {
                let mut rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
                match rows.next() {
                    Some(r) => Ok(r.unwrap_or_default() == "1"),
                    None => Ok(false),
                }
            })
            .unwrap_or(false);

        if migration_done {
            return Ok(0);
        }

        let mut stmt = conn
            .prepare("SELECT id, secret FROM webhook_endpoints WHERE secret IS NOT NULL")
            .map_err(|e| e.to_string())?;

        let rows: Vec<(i64, String)> = stmt
            .query_map([], |row| Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?)))
            .map_err(|e| e.to_string())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?;

        drop(stmt);

        let mut migrated = 0u64;
        let mut failed = Vec::new();

        // Use a single transaction — all-or-nothing migration
        conn.execute_batch("BEGIN IMMEDIATE")
            .map_err(|e| e.to_string())?;

        for (id, secret) in &rows {
            if !is_encrypted_fn(secret) {
                match encrypt_fn(secret) {
                    Ok(encrypted) => {
                        conn.execute(
                            "UPDATE webhook_endpoints SET secret = ?1 WHERE id = ?2",
                            params![encrypted, id],
                        )
                        .map_err(|e| {
                            let _ = conn.execute_batch("ROLLBACK");
                            format!("DB update failed: {}", e)
                        })?;
                        migrated += 1;
                    }
                    Err(e) => {
                        failed.push(format!("webhook {}: {}", id, e));
                    }
                }
            }
        }

        if !failed.is_empty() {
            let _ = conn.execute_batch("ROLLBACK");
            return Err(format!(
                "Migration failed for {} secrets: {}",
                failed.len(),
                failed.join("; ")
            ));
        }

        // Mark migration as complete
        conn.execute(
            "INSERT OR REPLACE INTO settings (key, value) VALUES ('webhook_secret_migration_done', '1')",
            [],
        )
        .map_err(|e| {
            let _ = conn.execute_batch("ROLLBACK");
            format!("Failed to mark migration done: {}", e)
        })?;

        conn.execute_batch("COMMIT")
            .map_err(|e| format!("Commit failed: {}", e))?;

        if migrated > 0 {
            log::info!(
                "Migrated {} plaintext webhook secrets to encrypted",
                migrated
            );
        }
        Ok(migrated)
    }

    // ==================== PHASE 2: BLAME VIEW ====================

    /// Get blame data for a file: maps each line to the scan that last introduced/changed it.
    /// Compares the snapshot content against the previous snapshot to find when each line changed.
    pub fn get_blame_data(&self, file_path: &str) -> Result<Vec<BlameLine>> {
        let snapshots = self.get_snapshots_for_file(file_path)?;
        if snapshots.is_empty() {
            return Ok(vec![]);
        }

        // Get content for each snapshot (newest first)
        let mut snapshot_contents: Vec<(i64, String, String)> = Vec::new(); // (id, content, created_at)
        for snap in &snapshots {
            if let Some(content) = self.get_snapshot_content(snap.id)? {
                snapshot_contents.push((snap.id, content, snap.created_at.clone()));
            }
        }

        if snapshot_contents.is_empty() {
            return Ok(vec![]);
        }

        // snapshot_contents[0] is newest. We go oldest → newest to find blame.
        snapshot_contents.reverse();

        // Start with the oldest snapshot's lines, all blamed to that snapshot
        let mut blame_map: Vec<(String, Option<i64>, Option<String>)> = Vec::new(); // (content, batch_id, detected_at)

        let oldest_lines: Vec<String> = snapshot_contents[0].1.lines().map(|s| s.to_string()).collect();
        for line in oldest_lines {
            blame_map.push((line, Some(snapshot_contents[0].0), Some(snapshot_contents[0].2.clone())));
        }

        // For each subsequent snapshot, compare line by line
        for (snap_id, content, created_at) in snapshot_contents.iter().skip(1) {
            let new_lines: Vec<String> = content.lines().map(|s| s.to_string()).collect();

            let new_blame = lcs_blame(&blame_map, &new_lines, *snap_id, created_at.clone());
            blame_map = new_blame;
        }

        // Reverse back to newest first and return
        blame_map.reverse();
        let mut result: Vec<BlameLine> = blame_map.into_iter().enumerate().map(|(i, (content, batch_id, detected_at))| {
            BlameLine {
                line_number: i + 1,
                content,
                change_type: if batch_id.is_some() { "known" } else { "unknown" }.to_string(),
                scan_batch_id: batch_id,
                detected_at,
            }
        }).collect();
        // Reverse so line 1 is at top (oldest first for blame display)
        result.reverse();
        // Re-number
        for (i, line) in result.iter_mut().enumerate() {
            line.line_number = i + 1;
        }
        Ok(result)
    }

    // ==================== PHASE 2: CHANGELOG GENERATOR ====================

    /// Generate changelog entries from scan batches
    pub fn get_changelog_entries(&self, limit: i64) -> Result<Vec<ChangelogEntry>> {
        let conn = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        let mut stmt = conn.prepare(
            "SELECT id, started_at, folders_scanned, total_files, new_files, modified_files, deleted_files, moved_files
             FROM scan_batches
             WHERE completed_at IS NOT NULL
             ORDER BY started_at DESC
             LIMIT ?1",
        )?;
        let batch_rows: Vec<(i64, String, String, i64, i64, i64, i64, i64)> = stmt.query_map(params![limit], |row| {
            Ok((
                row.get(0)?,
                row.get(1)?,
                row.get(2)?,
                row.get(3)?,
                row.get(4)?,
                row.get(5)?,
                row.get(6)?,
                row.get(7)?,
            ))
        })?.collect::<Result<Vec<_>>>()?;

        let mut entries = Vec::new();
        for (batch_id, started_at, folders_scanned, total_files, new_files, modified_files, deleted_files, moved_files) in batch_rows {
            // Get changes for this batch
            let mut change_stmt = conn.prepare(
                "SELECT c.id, c.file_id, f.path, f.filename, c.change_type, c.detected_at, c.previous_path, c.new_path
                 FROM changes c
                 JOIN files f ON c.file_id = f.id
                 WHERE c.scan_batch_id = ?1
                 ORDER BY c.detected_at DESC",
            )?;
            let changes = change_stmt.query_map(params![batch_id], |row| {
                Ok(ChangeRecord {
                    id: row.get(0)?,
                    file_id: row.get(1)?,
                    file_path: row.get(2)?,
                    filename: row.get(3)?,
                    change_type: row.get(4)?,
                    detected_at: row.get(5)?,
                    previous_path: row.get(6)?,
                    new_path: row.get(7)?,
                })
            })?.collect::<Result<Vec<_>>>()?;

            entries.push(ChangelogEntry {
                date: started_at,
                batch_id,
                folders_scanned,
                total_files,
                new_files,
                modified_files,
                deleted_files,
                moved_files,
                changes,
            });
        }
        Ok(entries)
    }

    // ==================== PHASE 2: SNAPSHOT COMPARE ====================

    /// Compare any two snapshots and return their content as a tuple (snap_a, snap_b)
    pub fn compare_any_snapshots(&self, id_a: i64, id_b: i64) -> Result<Option<(String, String, String, String)>> {
        let conn = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        let a = conn.query_row(
            "SELECT snapshot_path, original_filename, created_at FROM file_snapshots WHERE id = ?1",
            params![id_a],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?, row.get::<_, String>(2)?)),
        )?;
        let b = conn.query_row(
            "SELECT snapshot_path, original_filename, created_at FROM file_snapshots WHERE id = ?1",
            params![id_b],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?, row.get::<_, String>(2)?)),
        )?;

        // Decompress both
        let content_a = read_zstd_file(&a.0);
        let content_b = read_zstd_file(&b.0);

        // Return (filename_a, created_at_a, content_a, filename_b, created_at_b, content_b)
        // But tuple struct limit... return as 4 strings: (content_a, meta_a, content_b, meta_b)
        let meta_a = format!("{}|{}", a.1, a.2);
        let meta_b = format!("{}|{}", b.1, b.2);
        Ok(Some((content_a, meta_a, content_b, meta_b)))
    }

    // ==================== PHASE 3: FILE TYPE ANALYTICS ====================

    /// Get file extension statistics from all tracked files (top 50 by count)
    pub fn get_extension_stats(&self) -> Result<Vec<ExtensionStat>> {
        let conn = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        let mut stmt = conn.prepare(
            "SELECT COALESCE(extension, '(none)'), COUNT(*), COALESCE(SUM(size), 0)
             FROM files WHERE is_deleted = 0
             GROUP BY extension ORDER BY count(*) DESC LIMIT 50",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(ExtensionStat {
                extension: row.get(0)?,
                count: row.get(1)?,
                total_size: row.get(2)?,
            })
        })?;
        rows.collect::<Result<Vec<_>>>()
    }

    /// Get daily change trends for last N days
    pub fn get_daily_trends(&self, days: i64) -> Result<Vec<DailyTrend>> {
        let conn = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        let mut stmt = conn.prepare(
            "SELECT DATE(c.detected_at) as date,
                    SUM(CASE WHEN c.change_type = 'NEW' THEN 1 ELSE 0 END),
                    SUM(CASE WHEN c.change_type = 'MODIFIED' THEN 1 ELSE 0 END),
                    SUM(CASE WHEN c.change_type = 'DELETED' THEN 1 ELSE 0 END),
                    SUM(CASE WHEN c.change_type = 'MOVED' THEN 1 ELSE 0 END)
             FROM changes c
             WHERE c.detected_at >= DATE('now', '-' || ?1 || ' days')
             GROUP BY DATE(c.detected_at)
             ORDER BY date ASC",
        )?;
        let rows = stmt.query_map(params![days], |row| {
            Ok(DailyTrend {
                date: row.get(0)?,
                new_count: row.get(1)?,
                modified_count: row.get(2)?,
                deleted_count: row.get(3)?,
                moved_count: row.get(4)?,
            })
        })?;
        rows.collect::<Result<Vec<_>>>()
    }

    // ==================== PHASE 3: ADVANCED SEARCH ====================

    /// Advanced search with multiple filter parameters
    pub fn advanced_search(
        &self,
        query: Option<&str>,
        change_type: Option<&str>,
        date_from: Option<&str>,
        date_to: Option<&str>,
        extension: Option<&str>,
        min_size: Option<i64>,
        max_size: Option<i64>,
        limit: i64,
        offset: i64,
    ) -> Result<AdvancedSearchResult> {
        let conn = self.conn.lock().unwrap_or_else(|e| e.into_inner());

        // Build dynamic WHERE clause
        let mut conditions = Vec::new();
        let mut param_values: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();

        if let Some(q) = query {
            if !q.is_empty() {
                conditions.push("(LOWER(f.path) LIKE ? OR LOWER(f.filename) LIKE ?)");
                let escaped = q.to_lowercase().replace('\\', "\\\\").replace('%', "\\%").replace('_', "\\_");
                let pat = format!("%{}%", escaped);
                param_values.push(Box::new(pat.clone()));
                param_values.push(Box::new(pat));
            }
        }

        if let Some(ct) = change_type {
            if !ct.is_empty() && ct != "ALL" {
                conditions.push("c.change_type = ?");
                param_values.push(Box::new(ct.to_string()));
            }
        }

        if let Some(df) = date_from {
            if !df.is_empty() {
                conditions.push("DATE(c.detected_at) >= ?");
                param_values.push(Box::new(df.to_string()));
            }
        }

        if let Some(dt) = date_to {
            if !dt.is_empty() {
                conditions.push("DATE(c.detected_at) <= ?");
                param_values.push(Box::new(dt.to_string()));
            }
        }

        if let Some(ext) = extension {
            if !ext.is_empty() {
                conditions.push("LOWER(f.extension) = ?");
                param_values.push(Box::new(ext.to_lowercase()));
            }
        }

        if let Some(min_s) = min_size {
            conditions.push("f.size >= ?");
            param_values.push(Box::new(min_s));
        }

        if let Some(max_s) = max_size {
            conditions.push("f.size <= ?");
            param_values.push(Box::new(max_s));
        }

        let where_clause = if conditions.is_empty() {
            String::new()
        } else {
            format!("WHERE {}", conditions.join(" AND "))
        };

        // Count query
        let count_sql = format!(
            "SELECT COUNT(*) FROM changes c JOIN files f ON c.file_id = f.id {}",
            where_clause
        );
        let mut count_stmt = conn.prepare(&count_sql)?;
        let total_count: i64 = {
            let param_refs: Vec<&dyn rusqlite::types::ToSql> = param_values.iter().map(|p| p.as_ref()).collect();
            count_stmt.query_row(param_refs.as_slice(), |row| row.get(0))?
        };

        // Data query
        let data_sql = format!(
            "SELECT c.id, c.file_id, f.path, f.filename, c.change_type, c.detected_at, c.previous_path, c.new_path
             FROM changes c JOIN files f ON c.file_id = f.id {}
             ORDER BY c.detected_at DESC LIMIT ? OFFSET ?",
            where_clause
        );

        let mut data_stmt = conn.prepare(&data_sql)?;
        // Add limit and offset params
        param_values.push(Box::new(limit));
        param_values.push(Box::new(offset));

        let param_refs: Vec<&dyn rusqlite::types::ToSql> = param_values.iter().map(|p| p.as_ref()).collect();
        let records = data_stmt.query_map(param_refs.as_slice(), |row| {
            Ok(ChangeRecord {
                id: row.get(0)?,
                file_id: row.get(1)?,
                file_path: row.get(2)?,
                filename: row.get(3)?,
                change_type: row.get(4)?,
                detected_at: row.get(5)?,
                previous_path: row.get(6)?,
                new_path: row.get(7)?,
            })
        })?.collect::<Result<Vec<_>>>()?;

        Ok(AdvancedSearchResult { records, total_count })
    }

    // ==================== PHASE 3: EXPORT DATA ====================

    /// Get all data needed for a comprehensive export
    pub fn get_export_data(&self) -> Result<ExportData> {
        let summary = self.get_change_stats_today()?;
        let batches = self.get_changelog_entries(30)?;
        let extension_stats = self.get_extension_stats()?;
        let trends = self.get_daily_trends(90)?;
        let snapshots = self.get_all_file_snapshots(100)?;
        let duplicate_groups = self.get_duplicate_groups()?;
        let monitored_folders = self.get_monitored_folders()?;

        Ok(ExportData {
            generated_at: chrono::Local::now().to_rfc3339(),
            summary,
            batches,
            extension_stats,
            trends,
            snapshots,
            duplicate_groups,
            monitored_folders,
        })
    }

    /// Generate CSV of all changes
    pub fn export_changes_csv(&self, date_from: Option<&str>, date_to: Option<&str>) -> Result<String> {
        let conn = self.conn.lock().unwrap_or_else(|e| e.into_inner());
        let mut conditions = Vec::new();
        let mut param_values: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();

        if let Some(df) = date_from {
            conditions.push("DATE(c.detected_at) >= ?");
            param_values.push(Box::new(df.to_string()));
        }
        if let Some(dt) = date_to {
            conditions.push("DATE(c.detected_at) <= ?");
            param_values.push(Box::new(dt.to_string()));
        }

        let where_clause = if conditions.is_empty() {
            String::new()
        } else {
            format!("WHERE {}", conditions.join(" AND "))
        };

        let sql = format!(
            "SELECT c.id, f.path, f.filename, COALESCE(f.extension, ''), f.size, c.change_type, c.detected_at, c.previous_path, c.new_path
             FROM changes c JOIN files f ON c.file_id = f.id {}
             ORDER BY c.detected_at DESC",
            where_clause
        );

        let mut stmt = conn.prepare(&sql)?;
        let param_refs: Vec<&dyn rusqlite::types::ToSql> = param_values.iter().map(|p| p.as_ref()).collect();

        let mut csv = String::from("ID,Path,Filename,Extension,Size,Change Type,Detected At,Previous Path,New Path\n");
        let rows = stmt.query_map(param_refs.as_slice(), |row| {
            let id: i64 = row.get(0)?;
            let path: String = row.get(1)?;
            let filename: String = row.get(2)?;
            let ext: String = row.get(3)?;
            let size: i64 = row.get(4)?;
            let change_type: String = row.get(5)?;
            let detected_at: String = row.get(6)?;
            let prev: Option<String> = row.get(7)?;
            let new: Option<String> = row.get(8)?;

            // Escape CSV fields — handles formula injection (prefixes =, +, -, @)
            let escape = |s: &str| {
                let escaped = if s.contains(',') || s.contains('"') || s.contains('\n') {
                    format!("\"{}\"", s.replace('"', "\"\""))
                } else {
                    s.to_string()
                };
                // Prefix with single quote if starts with dangerous character (CSV formula injection)
                if escaped.starts_with('=') || escaped.starts_with('+')
                    || escaped.starts_with('-') || escaped.starts_with('@')
                {
                    format!("'{}", escaped)
                } else {
                    escaped
                }
            };

            Ok(format!(
                "{},{},{},{},{},{},{},{},{}",
                id,
                escape(&path),
                escape(&filename),
                escape(&ext),
                size,
                escape(&change_type),
                escape(&detected_at),
                escape(&prev.unwrap_or_default()),
                escape(&new.unwrap_or_default()),
            ))
        })?;

        for row in rows {
            csv.push_str(&row?);
            csv.push('\n');
        }

        Ok(csv)
    }
}

// --- Glob matcher using the glob crate ---

fn glob_match(pattern: &str, path: &str) -> bool {
    // Use the glob crate for correct glob semantics (supports *, ?, **)
    match glob::Pattern::new(&pattern.to_lowercase()) {
        Ok(pat) => pat.matches(&path.to_lowercase()),
        Err(_) => {
            // Fallback to simple contains if pattern is invalid
            path.to_lowercase().contains(&pattern.to_lowercase())
        }
    }
}

/// Simple regex-like pattern matching for common patterns
/// Supports: literal text, dot (any char), star (zero or more), question mark (one char)
fn regex_simple(pattern: &str) -> Result<RegexSimple, String> {
    Ok(RegexSimple { pattern: pattern.to_lowercase() })
}

struct RegexSimple {
    pattern: String,
}

impl RegexSimple {
    fn is_match(&self, text: &str) -> bool {
        let text_lower = text.to_lowercase();
        simple_regex_match(&self.pattern, &text_lower)
    }
}

/// Simple regex matching: supports '.', '*', '?' on top of literal matching
/// Max 10 wildcards ('*') to prevent exponential time complexity.
/// Uses an iteration budget to abort pathological patterns.
fn simple_regex_match(pattern: &str, text: &str) -> bool {
    let p: Vec<char> = pattern.chars().collect();
    let t: Vec<char> = text.chars().collect();
    // Guard: limit wildcard count to prevent ReDoS
    if p.iter().filter(|&&c| c == '*').count() > 10 {
        return false;
    }
    let mut budget: u64 = 4096; // iteration budget
    regex_match_recursive(&p, &t, &mut budget)
}

fn regex_match_recursive(pattern: &[char], text: &[char], budget: &mut u64) -> bool {
    if *budget == 0 {
        return false; // abort on suspected pathological input
    }
    *budget -= 1;

    if pattern.is_empty() {
        return text.is_empty();
    }

    if pattern[0] == '*' {
        // Star: match zero or more of any char
        // Try matching remaining pattern at every position in text
        for i in 0..=text.len() {
            if regex_match_recursive(&pattern[1..], &text[i..], budget) {
                return true;
            }
        }
        return false;
    }

    if text.is_empty() {
        return false;
    }

    if pattern[0] == '?' || pattern[0] == text[0] {
        return regex_match_recursive(&pattern[1..], &text[1..], budget);
    }

    false
}

/// Read and decompress a zstd-compressed file, returning the UTF-8 string content.
/// Caps decompressed output at 50MB to prevent zip-bomb OOM.
fn read_zstd_file(path: &str) -> String {
    let p = std::path::Path::new(path);
    if !p.exists() {
        return String::new();
    }
    let compressed = match std::fs::read(p) {
        Ok(c) => c,
        Err(_) => return String::new(),
    };
    let mut decoder = match zstd::Decoder::new(&compressed[..]) {
        Ok(d) => d,
        Err(_) => return String::new(),
    };
    let mut content = String::new();
    // Limit decompressed size to 50MB
    const MAX_DECOMPRESSED: usize = 50 * 1024 * 1024;
    let mut buf = [0u8; 8192];
    loop {
        let n = match std::io::Read::read(&mut decoder, &mut buf) {
            Ok(0) => break,
            Ok(n) => n,
            Err(_) => break,
        };
        if content.len() + n > MAX_DECOMPRESSED {
            log::warn!("read_zstd_file: decompressed content exceeds 50MB limit, truncating");
            break;
        }
        content.push_str(match std::str::from_utf8(&buf[..n]) {
            Ok(s) => s,
            Err(_) => break,
        });
    }
    content
}

/// LCS-based blame: compares old blame map against new_lines.
/// Lines present in old keep their old blame info. New/changed lines get the new snapshot info.
fn lcs_blame(
    old_blame: &[(String, Option<i64>, Option<String>)],
    new_lines: &[String],
    new_snap_id: i64,
    new_detected_at: String,
) -> Vec<(String, Option<i64>, Option<String>)> {
    let n = old_blame.len();
    let m = new_lines.len();

    // Guard against O(n*m) memory blowup for large files
    // If combined lines exceed 5000, fall back to simple line-by-line assignment
    if n > 2500 || m > 2500 {
        // O(n+m) fallback: assume all lines are new unless they match at same position
        return new_lines.iter().enumerate().map(|(idx, line)| {
            if idx < old_blame.len() && old_blame[idx].0 == *line {
                old_blame[idx].clone()
            } else {
                (line.clone(), Some(new_snap_id), Some(new_detected_at.clone()))
            }
        }).collect();
    }

    // Build the old line strings for LCS comparison
    let old_line_strings: Vec<&str> = old_blame.iter().map(|(l, _, _)| l.as_str()).collect();
    let new_line_strings: Vec<&str> = new_lines.iter().map(|s| s.as_str()).collect();

    // Compute LCS table
    let mut dp = vec![vec![0i32; m + 1]; n + 1];
    for i in 1..=n {
        for j in 1..=m {
            if old_line_strings[i - 1] == new_line_strings[j - 1] {
                dp[i][j] = dp[i - 1][j - 1] + 1;
            } else {
                dp[i][j] = std::cmp::max(dp[i - 1][j], dp[i][j - 1]);
            }
        }
    }

    // Backtrack to find which new lines come from old (matched) or are new
    let mut i = n;
    let mut j = m;
    let mut new_result: Vec<(String, Option<i64>, Option<String>)> = Vec::new();

    while i > 0 || j > 0 {
        if i > 0 && j > 0 && old_line_strings[i - 1] == new_line_strings[j - 1] {
            // This line matches — inherit blame from old
            new_result.push(old_blame[i - 1].clone());
            i -= 1;
            j -= 1;
        } else if j > 0 && (i == 0 || dp[i][j - 1] >= dp[i - 1][j]) {
            // New line (added in new snapshot)
            new_result.push((new_lines[j - 1].clone(), Some(new_snap_id), Some(new_detected_at.clone())));
            j -= 1;
        } else if i > 0 {
            // Removed line from old — skip (not in new_lines)
            i -= 1;
        }
    }

    new_result.reverse();
    new_result
}
