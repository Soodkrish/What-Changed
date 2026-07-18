use std::collections::HashMap;
use std::sync::Arc;
use sha2::Sha256;
use hmac::{Hmac, Mac};
use crate::database::Database;
use crate::crypto::CryptoManager;

/// Write a diagnostic line to a file in the user's temp directory so we can
/// trace the webhook flow even when `npx tauri dev` swallows stdout.
/// The file is at: <app_data_dir>/webhook_debug.log
/// If `app_data_dir` is None, falls back to writing next to the exe.
fn debug_log(app_data_dir: Option<&std::path::Path>, msg: &str) {
    let path = match app_data_dir {
        Some(dir) => dir.join("webhook_debug.log"),
        None => {
            // Fallback: write to current dir
            std::path::PathBuf::from("webhook_debug.log")
        }
    };
    use std::io::Write;
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
    {
        let ts = chrono::Local::now().format("%Y-%m-%d %H:%M:%S%.3f").to_string();
        let _ = writeln!(f, "[{}] {}", ts, msg);
    }
}

/// Check if an IP address string is disallowed (private, loopback, link-local, or unspecified).
pub fn is_ip_disallowed(ip: &str) -> bool {
    let normalized: std::net::IpAddr = match ip.parse() {
        Ok(addr) => addr,
        Err(_) => return false, // hostname (not an IP) — allow, DNS rebinding check happens at fire time
    };

    let ipv4: std::net::Ipv4Addr = match normalized {
        std::net::IpAddr::V4(v4) => v4,
        std::net::IpAddr::V6(v6) => {
            match v6.to_ipv4_mapped() {
                Some(v4) => v4,
                None => return v6.is_loopback() || v6.is_unspecified(),
            }
        }
    };

    ipv4.is_loopback()
        || ipv4.is_unspecified()
        || ipv4.is_link_local()
        || ipv4.is_private()
}

/// Re-resolve a URL's hostname at fire time and verify all resolved IPs are allowed.
/// Returns Ok(true) if safe, Ok(false) if any IP is disallowed (DNS rebinding detected).
pub fn verify_dns_at_fire_time(url: &str) -> Result<bool, String> {
    let parsed = url::Url::parse(url)
        .map_err(|e| format!("Invalid URL: {}", e))?;
    let host = parsed.host_str()
        .ok_or_else(|| "No hostname in URL".to_string())?;
    let port = parsed.port_or_known_default().unwrap_or(80);
    let addr = format!("{}:{}", host, port);

    use std::net::ToSocketAddrs;
    match addr.to_socket_addrs() {
        Ok(addrs) => {
            for addr in addrs {
                let ip = addr.ip().to_string();
                if is_ip_disallowed(&ip) {
                    log::warn!(
                        "DNS rebinding detected for {}: resolved to {} (disallowed)",
                        url, ip
                    );
                    return Ok(false);
                }
            }
            Ok(true)
        }
        Err(e) => {
            log::warn!("DNS resolution failed for {}: {}", host, e);
            Err(format!("Cannot resolve hostname: {}", e))
        }
    }
}

type HmacSha256 = Hmac<Sha256>;

/// Maximum number of changes to include in a single webhook payload
/// to avoid overwhelming receivers with huge batches.
pub const MAX_CHANGES_PER_WEBHOOK: usize = 100;

/// Result of firing webhooks for a set of changes.
pub struct WebhookFireReport {
    pub fired: u64,
    pub failed: u64,
    pub errors: Vec<String>,
}

/// Compute HMAC-SHA256 signature of the payload body using the webhook secret.
///
/// This is proper HMAC — NOT a plain SHA256 hash of the secret.
/// The receiver should verify: HMAC-SHA256(secret, body) == signature.
pub fn compute_signature(secret: &str, body: &str) -> String {
    let mut mac = HmacSha256::new_from_slice(secret.as_bytes())
        .expect("HMAC can take key of any size");
    mac.update(body.as_bytes());
    let result = mac.finalize();
    hex::encode(result.into_bytes())
}

/// Verify an HMAC-SHA256 signature against a payload body.
/// Uses constant-time comparison via hmac::Mac::verify_slice() to prevent timing attacks.
pub fn verify_signature(secret: &str, body: &str, signature: &str) -> bool {
    let mut mac = HmacSha256::new_from_slice(secret.as_bytes())
        .expect("HMAC can take key of any size");
    mac.update(body.as_bytes());
    // Decode the hex signature and verify in constant time
    if let Ok(sig_bytes) = hex::decode(signature) {
        mac.verify_slice(&sig_bytes).is_ok()
    } else {
        false
    }
}

/// Fire webhooks for a set of changes. Called server-side after scan completion.
///
/// Optimizations vs the old implementation:
/// - Fetches endpoints ONCE per event type, not per-change
/// - Uses proper HMAC-SHA256 (signs the payload, not just the secret)
/// - Batch-caps payloads at MAX_CHANGES_PER_WEBHOOK
/// - Uses shared reqwest client (no per-fire TLS pool creation)
pub async fn fire_webhooks_for_changes(
    db: &Arc<Database>,
    crypto: &Arc<CryptoManager>,
    http_client: &reqwest::Client,
    changes_json: &str,
    app_data_dir: Option<&std::path::Path>,
) -> Result<WebhookFireReport, String> {
    debug_log(app_data_dir, &format!("=== WEBHOOK FIRE START === changes_json len={}", changes_json.len()));
    log::info!("=== WEBHOOK FIRE START === changes_json len={}", changes_json.len());

    // Parse and cap the changes
    let mut changes: Vec<serde_json::Value> = serde_json::from_str(changes_json)
        .map_err(|e| format!("Invalid changes JSON: {}", e))?;

    log::info!("Parsed {} changes from JSON", changes.len());
    debug_log(app_data_dir, &format!("Parsed {} changes from JSON", changes.len()));

    if changes.len() > MAX_CHANGES_PER_WEBHOOK {
        log::warn!(
            "Webhook payload capped from {} to {} changes",
            changes.len(),
            MAX_CHANGES_PER_WEBHOOK
        );
        changes.truncate(MAX_CHANGES_PER_WEBHOOK);
    }

    if changes.is_empty() {
        log::warn!("WEBHOOK: No changes after parsing — returning early");
        debug_log(app_data_dir, "No changes after parsing — returning early");
        return Ok(WebhookFireReport {
            fired: 0,
            failed: 0,
            errors: vec![],
        });
    }

    // Collect event types from the changes
    let event_types: Vec<String> = changes
        .iter()
        .filter_map(|c| c.get("change_type").and_then(|v| v.as_str()).map(|s| s.to_uppercase()))
        .collect();

    log::info!("Event types found: {:?}", event_types);
    debug_log(app_data_dir, &format!("Event types found: {:?}", event_types));

    // Fetch endpoints ONCE per event type (not per-change)
    let mut endpoints_by_event: HashMap<String, Vec<_>> = HashMap::new();
    let mut seen_events = std::collections::HashSet::new();

    // Also check: how many total webhook endpoints exist, and how many are enabled?
    match db.get_all_webhook_endpoints() {
        Ok(all_endpoints) => {
            let enabled_count = all_endpoints.iter().filter(|e| e.enabled).count();
            log::info!("WEBHOOK: Total endpoints={}, enabled={}", all_endpoints.len(), enabled_count);
            debug_log(app_data_dir, &format!("Total endpoints={}, enabled={}", all_endpoints.len(), enabled_count));
            for ep in &all_endpoints {
                let ep_info = format!("  Endpoint: id={} name='{}' events='{}' enabled={} url={}",
                    ep.id, ep.name, ep.events, ep.enabled, ep.url);
                log::info!("{}", ep_info);
                debug_log(app_data_dir, &ep_info);
            }
        }
        Err(e) => {
            log::error!("WEBHOOK: Failed to list all endpoints: {}", e);
            debug_log(app_data_dir, &format!("Failed to list all endpoints: {}", e));
        }
    }

    for event_type in &event_types {
        if seen_events.contains(event_type) {
            continue;
        }
        seen_events.insert(event_type.clone());
        match db.get_active_webhooks_for_event(event_type) {
            Ok(endpoints) => {
                log::info!("WEBHOOK: event '{}' matched {} endpoints", event_type, endpoints.len());
                debug_log(app_data_dir, &format!("Event '{}' matched {} endpoints", event_type, endpoints.len()));
                for ep in &endpoints {
                    let m = format!("  Matched: id={} name='{}' events='{}'", ep.id, ep.name, ep.events);
                    log::info!("{}", m);
                    debug_log(app_data_dir, &m);
                }
                endpoints_by_event.insert(event_type.clone(), endpoints);
            }
            Err(e) => {
                log::error!("Failed to fetch webhooks for event '{}': {}", event_type, e);
            }
        }
    }

    let mut fired = 0u64;
    let mut failed = 0u64;
    let mut errors = Vec::new();

    // Group changes by event type and fire each endpoint once per batch
    for (event_type, endpoints) in &endpoints_by_event {
        // Filter changes for this event type
        let batch_changes: Vec<&serde_json::Value> = changes
            .iter()
            .filter(|c| {
                c.get("change_type")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_uppercase() == *event_type)
                    .unwrap_or(false)
            })
            .collect();

        if batch_changes.is_empty() {
            continue;
        }

        // Build change summary text (shared across platforms)
        let change_lines: Vec<String> = batch_changes.iter().map(|c| {
            let ct = c.get("change_type").and_then(|v| v.as_str()).unwrap_or("?");
            let fp = c.get("file_path").and_then(|v| v.as_str()).unwrap_or("?");
            let fname = std::path::Path::new(fp)
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or(fp);
            let icon = match ct {
                "NEW" => "🟢",
                "MODIFIED" => "🔵",
                "DELETED" => "🔴",
                "MOVED" => "🟡",
                _ => "⚪",
            };
            format!("{} `{}` — {}", icon, fname, ct)
        }).collect();

        let summary = if change_lines.len() <= 5 {
            change_lines.join("\n")
        } else {
            let first_4 = change_lines[..4].join("\n");
            format!("{}\n\n…and {} more", first_4, change_lines.len() - 4)
        };

        for endpoint in endpoints {
            // DNS re-verify before fire (mitigate DNS rebinding TOCTOU)
            match verify_dns_at_fire_time(&endpoint.url) {
                Ok(true) => {}
                Ok(false) => {
                    failed += 1;
                    let msg = format!("{}: blocked (DNS rebinding detected)", endpoint.name);
                    log::warn!("{}", msg);
                    debug_log(app_data_dir, &msg);
                    errors.push(msg);
                    continue;
                }
                Err(e) => {
                    failed += 1;
                    let msg = format!("{}: DNS resolution failed — {}", endpoint.name, e);
                    log::warn!("{}", msg);
                    debug_log(app_data_dir, &msg);
                    errors.push(msg);
                    continue;
                }
            }

            // Decrypt secret if present
            let secret = if let Some(ref encrypted) = endpoint.secret {
                match crypto.decrypt(encrypted) {
                    Ok(s) => Some(s),
                    Err(e) => {
                        log::error!("Failed to decrypt webhook secret for {}: {}", endpoint.name, e);
                        errors.push(format!("{}: decrypt failed", endpoint.name));
                        failed += 1;
                        continue;
                    }
                }
            } else {
                None
            };

            // Build platform-specific payload
            let url_lower = endpoint.url.to_lowercase();
            let is_telegram = url_lower.contains("api.telegram.org");

            let body = if is_telegram {
                // Telegram: needs chat_id and text field
                // Extract chat_id from URL query param: ?chat_id=12345
                let chat_id = url::Url::parse(&endpoint.url)
                    .ok()
                    .and_then(|u| u.query_pairs().find(|(k, _)| k == "chat_id").map(|(_, v)| v.to_string()))
                    .unwrap_or_default();

                let header = format!("🔔 **What Changed?** — {} {}", event_type, batch_changes.len());
                let text = if chat_id.is_empty() {
                    format!("{}\n\n{}", header, summary)
                } else {
                    // chat_id is in URL query params, just build text
                    format!("{}\n\n{}", header, summary)
                };

                let mut payload = serde_json::json!({
                    "text": text,
                    "parse_mode": "Markdown",
                });
                // Add chat_id if we found it
                if !chat_id.is_empty() {
                    payload["chat_id"] = serde_json::json!(chat_id);
                }
                payload.to_string()
            } else {
                // Discord and everything else: content + username
                let header = format!("🔔 **What Changed?** — {} {} change{}", event_type, batch_changes.len(),
                    if batch_changes.len() == 1 { "" } else { "s" });
                let content = format!("{}\n\n{}", header, summary);

                serde_json::json!({
                    "content": content,
                    "username": "What Changed?",
                    "event": event_type,
                    "changes": batch_changes,
                    "count": batch_changes.len(),
                    "timestamp": chrono::Utc::now().to_rfc3339(),
                    "app": "What Changed?",
                }).to_string()
            };

            // Build the HTTP request
            log::info!("WEBHOOK: Sending to {} url={}", endpoint.name, endpoint.url);
            log::info!("WEBHOOK: Payload preview (first 200 chars): {}", &body[..body.len().min(200)]);
            debug_log(app_data_dir, &format!("Sending to {} url={}", endpoint.name, endpoint.url));
            let mut req = http_client
                .post(&endpoint.url)
                .header("Content-Type", "application/json")
                .header("User-Agent", "WhatChanged/1.0")
                .header("X-Webhook-App", "What Changed?")
                .header("X-Webhook-Event", event_type);

            // Add HMAC-SHA256 signature header if secret is present
            if let Some(ref secret_val) = secret {
                let signature = compute_signature(secret_val, &body);
                req = req.header("X-Webhook-Signature", format!("sha256={}", signature));
            }

            // Fire the webhook (with the body as the payload)
            log::info!("WEBHOOK: About to send HTTP POST...");
            debug_log(app_data_dir, "About to send HTTP POST...");
            match req.body(body.clone()).send().await {
                Ok(resp) => {
                    let status = resp.status().as_u16();
                    let is_success = (200..300).contains(&status);
                    log::info!("WEBHOOK: Got response status={} is_success={}", status, is_success);
                    debug_log(app_data_dir, &format!("Got response status={} is_success={}", status, is_success));
                    if is_success {
                        fired += 1;
                        log::info!(
                            "Webhook FIRED OK: {} → {} (status {})",
                            endpoint.name,
                            endpoint.url,
                            status
                        );
                        debug_log(app_data_dir, &format!("FIRED OK: {} → {} (status {})", endpoint.name, endpoint.url, status));
                    } else {
                        failed += 1;
                        // Read response body for error details
                        let resp_body = resp.text().await.unwrap_or_else(|_| "<could not read>".to_string());
                        let msg = format!(
                            "{}: returned status {} body={}",
                            endpoint.name, status, &resp_body[..resp_body.len().min(500)]
                        );
                        log::warn!("WEBHOOK FAILED: {}", msg);
                        debug_log(app_data_dir, &format!("WEBHOOK FAILED: {}", msg));
                        errors.push(msg);
                    }
                    let _ = db.update_webhook_trigger(endpoint.id, status as i64);
                }
                Err(e) => {
                    failed += 1;
                    let msg = format!("{}: {}", endpoint.name, e);
                    log::error!("Webhook fire failed: {}", msg);
                    debug_log(app_data_dir, &format!("HTTP SEND FAILED: {}", msg));
                    errors.push(msg);
                    let _ = db.update_webhook_trigger(endpoint.id, 0);
                }
            }
        }
    }

    // Store the payload as the latest report
    let _ = db.set_setting(
        "webhook_latest_report",
        &serde_json::json!({
            "fired": fired,
            "failed": failed,
            "errors": errors,
            "timestamp": chrono::Utc::now().to_rfc3339(),
        })
        .to_string(),
    );

    log::info!("=== WEBHOOK FIRE END === fired={}, failed={}, errors={:?}", fired, failed, errors);
    debug_log(app_data_dir, &format!("=== WEBHOOK FIRE END === fired={}, failed={}, errors={:?}", fired, failed, errors));

    Ok(WebhookFireReport {
        fired,
        failed,
        errors,
    })
}

/// Fire daily summary to all enabled webhook endpoints.
/// Called by the scheduler once per day at the user-configured time.
pub async fn fire_daily_summary_webhook(
    db: &Arc<Database>,
    crypto: &Arc<CryptoManager>,
    http_client: &reqwest::Client,
    app_data_dir: Option<&std::path::Path>,
) -> Result<WebhookFireReport, String> {
    debug_log(app_data_dir, "=== DAILY SUMMARY WEBHOOK FIRE ===");

    let stats = db.get_change_stats_today()
        .map_err(|e| format!("Failed to get today's stats: {}", e))?;

    let total = stats.new_count + stats.modified_count + stats.deleted_count + stats.moved_count;
    if total == 0 {
        log::info!("Daily summary: no changes today, skipping webhook");
        debug_log(app_data_dir, "Daily summary: no changes today, skipping");
        return Ok(WebhookFireReport { fired: 0, failed: 0, errors: vec![] });
    }

    // Build rich summary text
    let mut parts = Vec::new();
    if stats.new_count > 0 { parts.push(format!("🟢 {} new", stats.new_count)); }
    if stats.modified_count > 0 { parts.push(format!("🔵 {} modified", stats.modified_count)); }
    if stats.deleted_count > 0 { parts.push(format!("🔴 {} deleted", stats.deleted_count)); }
    if stats.moved_count > 0 { parts.push(format!("🟡 {} moved", stats.moved_count)); }

    // Get recent changes for the detailed list (up to 10)
    let changes = db.get_changes_today().unwrap_or_default();
    let detail_lines: Vec<String> = changes.iter().take(10).map(|c| {
        let icon = match c.change_type.as_str() {
            "NEW" => "🟢",
            "MODIFIED" => "🔵",
            "DELETED" => "🔴",
            "MOVED" => "🟡",
            _ => "⚪",
        };
        let fname = std::path::Path::new(&c.file_path)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or(&c.filename);
        format!("{} `{}`", icon, fname)
    }).collect();

    let today = chrono::Local::now().format("%B %d, %Y").to_string();

    // Fetch all enabled endpoints
    let endpoints = db.get_all_webhook_endpoints()
        .map_err(|e| format!("Failed to fetch endpoints: {}", e))?;

    let enabled: Vec<_> = endpoints.into_iter().filter(|e| e.enabled).collect();
    if enabled.is_empty() {
        log::info!("Daily summary: no enabled webhook endpoints");
        debug_log(app_data_dir, "Daily summary: no enabled endpoints");
        return Ok(WebhookFireReport { fired: 0, failed: 0, errors: vec![] });
    }

    log::info!("Daily summary: {} total changes, {} enabled endpoints", total, enabled.len());
    debug_log(app_data_dir, &format!("Daily summary: {} total changes, {} enabled endpoints", total, enabled.len()));

    let mut fired = 0u64;
    let mut failed = 0u64;
    let mut errors = Vec::new();

    for endpoint in &enabled {
        // DNS re-verify
        match verify_dns_at_fire_time(&endpoint.url) {
            Ok(true) => {}
            Ok(false) => {
                failed += 1;
                let msg = format!("{}: blocked (DNS rebinding)", endpoint.name);
                log::warn!("{}", msg);
                debug_log(app_data_dir, &msg);
                errors.push(msg);
                continue;
            }
            Err(e) => {
                failed += 1;
                let msg = format!("{}: DNS failed — {}", endpoint.name, e);
                log::warn!("{}", msg);
                debug_log(app_data_dir, &msg);
                errors.push(msg);
                continue;
            }
        }

        // Decrypt secret
        let secret = if let Some(ref encrypted) = endpoint.secret {
            match crypto.decrypt(encrypted) {
                Ok(s) => Some(s),
                Err(e) => {
                    log::error!("Failed to decrypt secret for {}: {}", endpoint.name, e);
                    errors.push(format!("{}: decrypt failed", endpoint.name));
                    failed += 1;
                    continue;
                }
            }
        } else {
            None
        };

        // Build platform-specific payload
        let is_telegram = endpoint.url.to_lowercase().contains("api.telegram.org");

        let header = format!("📊 **What Changed? — Daily Summary**\n{}", today);
        let stats_text = parts.join("  •  ");
        let detail_text = if detail_lines.len() < changes.len() {
            format!("{}\n\n{}\n\n…and {} more", detail_text_raw(&detail_lines), "", changes.len() - 10)
        } else {
            detail_text_raw(&detail_lines)
        };

        let body = if is_telegram {
            let text = format!("{}\n\n{}\n\n{}", header, stats_text, detail_text);
            serde_json::json!({
                "text": text,
                "parse_mode": "Markdown",
            }).to_string()
        } else {
            let content = format!("{}\n\n{}\n\n{}", header, stats_text, detail_text);
            serde_json::json!({
                "content": content,
                "username": "What Changed?",
                "event": "DAILY_SUMMARY",
                "stats": {
                    "new": stats.new_count,
                    "modified": stats.modified_count,
                    "deleted": stats.deleted_count,
                    "moved": stats.moved_count,
                    "total": total,
                },
                "date": today,
                "timestamp": chrono::Utc::now().to_rfc3339(),
                "app": "What Changed?",
            }).to_string()
        };

        log::info!("Daily summary: sending to {} ({})", endpoint.name, endpoint.url);
        debug_log(app_data_dir, &format!("Daily summary: sending to {}", endpoint.name));

        let mut req = http_client
            .post(&endpoint.url)
            .header("Content-Type", "application/json")
            .header("User-Agent", "WhatChanged/1.0")
            .header("X-Webhook-App", "What Changed?")
            .header("X-Webhook-Event", "DAILY_SUMMARY");

        if let Some(ref secret_val) = secret {
            let signature = compute_signature(secret_val, &body);
            req = req.header("X-Webhook-Signature", format!("sha256={}", signature));
        }

        match req.body(body).send().await {
            Ok(resp) => {
                let status = resp.status().as_u16();
                let is_success = (200..300).contains(&status);
                if is_success {
                    fired += 1;
                    log::info!("Daily summary sent: {} (status {})", endpoint.name, status);
                    debug_log(app_data_dir, &format!("Daily summary sent: {} (status {})", endpoint.name, status));
                } else {
                    failed += 1;
                    let resp_body = resp.text().await.unwrap_or_default();
                    let msg = format!("{}: status {} body={}", endpoint.name, status, &resp_body[..resp_body.len().min(500)]);
                    log::warn!("Daily summary failed: {}", msg);
                    debug_log(app_data_dir, &format!("Daily summary failed: {}", msg));
                    errors.push(msg);
                }
                let _ = db.update_webhook_trigger(endpoint.id, status as i64);
            }
            Err(e) => {
                failed += 1;
                let msg = format!("{}: {}", endpoint.name, e);
                log::error!("Daily summary HTTP failed: {}", msg);
                debug_log(app_data_dir, &format!("Daily summary HTTP failed: {}", msg));
                errors.push(msg);
                let _ = db.update_webhook_trigger(endpoint.id, 0);
            }
        }
    }

    log::info!("Daily summary webhook done: fired={}, failed={}", fired, failed);
    debug_log(app_data_dir, &format!("Daily summary webhook done: fired={}, failed={}", fired, failed));

    Ok(WebhookFireReport { fired, failed, errors })
}

fn detail_text_raw(lines: &[String]) -> String {
    if lines.is_empty() {
        "No details available.".to_string()
    } else {
        lines.join("\n")
    }
}
