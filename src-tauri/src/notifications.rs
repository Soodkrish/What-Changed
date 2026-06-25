use crate::database::Database;
use std::sync::Arc;

pub struct NotificationManager {
    db: Arc<Database>,
}

impl NotificationManager {
    pub fn new(db: Arc<Database>) -> Self {
        NotificationManager { db }
    }

    /// Build a daily summary notification string
    pub fn build_daily_summary(&self) -> Result<String, String> {
        let stats = self
            .db
            .get_change_stats_today()
            .map_err(|e| format!("DB error: {}", e))?;

        let total = stats.new_count + stats.modified_count + stats.deleted_count + stats.moved_count;

        if total == 0 {
            return Ok("No changes detected today.".to_string());
        }

        let mut parts = Vec::new();
        if stats.new_count > 0 {
            parts.push(format!("{} new", stats.new_count));
        }
        if stats.modified_count > 0 {
            parts.push(format!("{} modified", stats.modified_count));
        }
        if stats.deleted_count > 0 {
            parts.push(format!("{} deleted", stats.deleted_count));
        }
        if stats.moved_count > 0 {
            parts.push(format!("{} moved", stats.moved_count));
        }

        Ok(format!(
            "Today: {}",
            parts.join(", ")
        ))
    }

    /// Check if notifications are enabled in settings
    pub fn is_enabled(&self) -> bool {
        self.db
            .get_setting("notifications_enabled")
            .ok()
            .flatten()
            .map(|v| v == "true")
            .unwrap_or(true)
    }

    /// Check if daily summary is enabled
    pub fn is_daily_summary_enabled(&self) -> bool {
        self.db
            .get_setting("daily_summary_enabled")
            .ok()
            .flatten()
            .map(|v| v == "true")
            .unwrap_or(true)
    }
}
