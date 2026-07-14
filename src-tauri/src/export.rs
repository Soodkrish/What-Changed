use crate::database::Database;
use std::sync::Arc;

pub struct ReportExporter {
    db: Arc<Database>,
}

impl ReportExporter {
    pub fn new(db: Arc<Database>) -> Self {
        ReportExporter { db }
    }

    /// Export daily report as JSON
    pub fn export_daily_json(&self, date: &str) -> Result<String, String> {
        let changes = self.db.get_changes_range(
            &format!("{}T00:00:00", date),
            &format!("{}T23:59:59", date),
        ).map_err(|e| e.to_string())?;

        let stats = self.db.get_change_stats_today().map_err(|e| e.to_string())?;
        let recovery = self.db.get_recovery_stats().map_err(|e| e.to_string())?;
        let batches = self.db.get_all_batches_with_changes().map_err(|e| e.to_string())?;
        let duplicates = self.db.get_duplicate_groups().map_err(|e| e.to_string())?;
        let wasted: i64 = duplicates.iter().map(|d| d.file_size * (d.file_count - 1)).sum();

        let report = serde_json::json!({
            "app": "What Changed?",
            "version": "0.1.0",
            "date": date,
            "generated_at": chrono::Utc::now().to_rfc3339(),
            "changes": {
                "total": changes.len(),
                "stats": {
                    "new": stats.new_count,
                    "modified": stats.modified_count,
                    "deleted": stats.deleted_count,
                    "moved": stats.moved_count,
                },
                "records": changes.iter().map(|c| {
                    serde_json::json!({
                        "id": c.id,
                        "file": c.filename,
                        "path": c.file_path,
                        "type": c.change_type,
                        "detected_at": c.detected_at,
                        "previous_path": c.previous_path,
                        "new_path": c.new_path,
                    })
                }).collect::<Vec<_>>(),
            },
            "scan_batches": batches.len(),
            "duplicates": {
                "groups": duplicates.len(),
                "wasted_bytes": wasted,
            },
            "recovery": {
                "recycle_bin_files": recovery.recycle_bin_count,
                "snapshots": recovery.snapshot_count,
                "snapshot_size": recovery.total_snapshot_size,
                "cloud_folders": recovery.cloud_folders_count,
            },
        });

        self.db.log_recovery_action(
            "export",
            Some(&format!("{{\"format\":\"json\",\"date\":\"{}\",\"changes\":{}}}", date, changes.len())),
            true,
            None,
        ).ok();

        serde_json::to_string_pretty(&report).map_err(|e| e.to_string())
    }

    /// Export daily report as CSV
    pub fn export_daily_csv(&self, date: &str) -> Result<String, String> {
        let changes = self.db.get_changes_range(
            &format!("{}T00:00:00", date),
            &format!("{}T23:59:59", date),
        ).map_err(|e| e.to_string())?;

        let mut csv = String::from("timestamp,filename,path,change_type,previous_path,new_path\n");

        for change in &changes {
            let prev = change.previous_path.as_deref().unwrap_or("");
            let new_p = change.new_path.as_deref().unwrap_or("");
            csv.push_str(&format!(
                "\"{}\",\"{}\",\"{}\",\"{}\",\"{}\",\"{}\"\n",
                change.detected_at,
                escape_csv(&change.filename),
                escape_csv(&change.file_path),
                change.change_type,
                escape_csv(prev),
                escape_csv(new_p),
            ));
        }

        self.db.log_recovery_action(
            "export",
            Some(&format!("{{\"format\":\"csv\",\"date\":\"{}\",\"changes\":{}}}", date, changes.len())),
            true,
            None,
        ).ok();

        Ok(csv)
    }
}

fn escape_csv(s: &str) -> String {
    // Strip Unicode bidirectional override characters (U+202A-U+202E)
    let cleaned: String = s.chars().filter(|c| {
        !matches!(c, '\u{202A}'..='\u{202E}')
    }).collect();
    let needs_quoting = cleaned.starts_with('=')
        || cleaned.starts_with('+')
        || cleaned.starts_with('-')
        || cleaned.starts_with('@')
        || cleaned.starts_with('\t')
        || cleaned.contains(',')
        || cleaned.contains('"')
        || cleaned.contains('\n')
        || cleaned.contains('\r')
        || cleaned.contains('\t');
    if needs_quoting {
        format!("\"{}\"", cleaned.replace('"', "\"\""))
    } else {
        cleaned
    }
}
