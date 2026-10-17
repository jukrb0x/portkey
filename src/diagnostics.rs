//! Explicit sanitized diagnostic collection. Never reads hardware or exports raw records.
use serde_json::{json, Value};
use std::{
    collections::VecDeque,
    fs,
    io::{Read, Seek, SeekFrom},
    path::{Path, PathBuf},
};
const FILE_LIMIT: u64 = 65536;
const RECORD_LIMIT: usize = 256;
const LINE_LIMIT: usize = 4096;

pub fn preview(current: Value, path: Option<&Path>) -> Value {
    json!({
        "version":1,
        "application":{"name":"PortKey","version":env!("CARGO_PKG_VERSION"),"platform":std::env::consts::OS},
        "current": sanitize_current(&current),
        "history": collect_history(path)
    })
}

fn collect_history(path: Option<&Path>) -> Value {
    let mut history = json!({"availability":"not_configured","records":[],"files_read":0,"files_missing":0,"files_unavailable":0,"bytes_read":0,"records_rejected":0,"partial_records":0,"truncated":false,"reported_dropped":0,"limits":{"files":6,"bytes_per_file":65536,"total_bytes":393216,"records":256,"line_bytes":4096},"notes":[]});
    let Some(path) = path else {
        history["notes"] = json!(["history_not_configured"]);
        return history;
    };
    let mut notes = vec![
        "bounded_tail_snapshot",
        "concurrent_rotation_possible",
        "timestamps_may_span_sessions",
        "retention_limited_to_known_rotations",
    ];
    if !path.is_absolute() {
        history["availability"] = json!("unavailable");
        history["notes"] = json!(["invalid_log_path"]);
        return history;
    }
    let mut records = VecDeque::new();
    let (
        mut read,
        mut missing,
        mut unavailable,
        mut bytes_read,
        mut rejected,
        mut partial,
        mut dropped,
    ) = (0u64, 0u64, 0u64, 0u64, 0u64, 0u64, 0u64);
    let mut truncated = false;
    for index in (0..=5).rev() {
        let file_path = if index == 0 {
            path.to_path_buf()
        } else {
            let mut name = path.as_os_str().to_os_string();
            name.push(format!(".{index}"));
            PathBuf::from(name)
        };
        match fs::symlink_metadata(&file_path) {
            Ok(meta) if meta.file_type().is_file() => {}
            Ok(_) => {
                unavailable += 1;
                continue;
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                missing += 1;
                continue;
            }
            Err(_) => {
                unavailable += 1;
                continue;
            }
        }
        let snapshot = (|| -> std::io::Result<(Vec<u8>, u64)> {
            let mut file = fs::File::open(file_path)?;
            let meta = file.metadata()?;
            if !meta.is_file() {
                return Err(std::io::Error::other("not a regular log"));
            }
            let offset = meta.len().saturating_sub(FILE_LIMIT);
            file.seek(SeekFrom::Start(offset))?;
            let mut bytes = Vec::new();
            file.take(FILE_LIMIT).read_to_end(&mut bytes)?;
            Ok((bytes, offset))
        })();
        let Ok((bytes, offset)) = snapshot else {
            unavailable += 1;
            continue;
        };
        read += 1;
        bytes_read += bytes.len() as u64;
        let mut data = bytes.as_slice();
        if offset > 0 {
            truncated = true;
            if !notes.contains(&"byte_limit") {
                notes.push("byte_limit");
            }
            partial += 1;
            data = data
                .iter()
                .position(|b| *b == b'\n')
                .map_or(&[][..], |i| &data[i + 1..]);
        }
        if !data.is_empty() && data.last() != Some(&b'\n') {
            partial += 1;
            truncated = true;
            data = data
                .iter()
                .rposition(|b| *b == b'\n')
                .map_or(&[][..], |i| &data[..=i]);
        }
        for line in data.split(|b| *b == b'\n').filter(|line| !line.is_empty()) {
            let record = if line.len() > LINE_LIMIT {
                None
            } else {
                serde_json::from_slice::<Value>(line)
                    .ok()
                    .and_then(|v| sanitize_record(&v))
            };
            let Some(record) = record else {
                rejected += 1;
                truncated = true;
                continue;
            };
            dropped = dropped.saturating_add(
                record
                    .get("dropped_count")
                    .and_then(Value::as_u64)
                    .unwrap_or(0),
            );
            if records.len() == RECORD_LIMIT {
                records.pop_front();
                truncated = true;
                if !notes.contains(&"record_limit") {
                    notes.push("record_limit");
                }
            }
            records.push_back(record);
        }
    }
    if missing > 0 {
        notes.push("missing_files");
    }
    if unavailable > 0 {
        notes.push("files_unavailable");
    }
    if rejected > 0 {
        notes.push("malformed_or_unallowlisted_records");
    }
    if partial > 0 {
        notes.push("partial_lines");
    }
    if dropped > 0 {
        notes.push("reported_log_drops");
        truncated = true;
    }
    history["availability"] = json!(if !records.is_empty() {
        "available"
    } else if read > 0 {
        "empty"
    } else {
        "unavailable"
    });
    history["records"] = json!(records);
    history["files_read"] = json!(read);
    history["files_missing"] = json!(missing);
    history["files_unavailable"] = json!(unavailable);
    history["bytes_read"] = json!(bytes_read);
    history["records_rejected"] = json!(rejected);
    history["partial_records"] = json!(partial);
    history["truncated"] = json!(truncated);
    history["reported_dropped"] = json!(dropped);
    history["notes"] = json!(notes);
    history
}
fn sanitize_record(source: &Value) -> Option<Value> {
    let event = source.get("event")?.as_str()?;
    if ![
        "session_start",
        "inventory_publish",
        "inventory_discarded",
        "usb_inventory_begin",
        "usb_inventory_end_ok",
        "usb_inventory_end_error",
        "inventory_begin",
        "inventory_end_ok",
        "inventory_end_error",
        "dispatch_queued",
        "dispatch_dequeued",
        "ownership_observed",
        "retry_wait",
        "write_attempt_begin",
        "write_attempt_confirmed",
        "write_attempt_unverified",
        "write_attempt_error",
        "completion_ok",
        "completion_error",
        "device_change",
        "usb_notifications_ready",
        "usb_notifications_unavailable",
        "usb_snapshot_retry",
        "helper_start",
        "helper_end",
        "helper_failed",
        "ddc_command",
        "trace_dropped",
        "add_type",
        "script_total",
        "native_helper_total",
        "usb_enumerate",
        "names_display_enumerate",
        "native_display_enumerate",
        "exact_display_open",
        "set_vcp",
        "postwrite_wait",
        "postwrite_readback",
        "read_input",
    ]
    .contains(&event)
    {
        return None;
    }
    let role = source.get("role")?.as_str()?;
    if ![
        "controller",
        "inventory",
        "native",
        "automatic",
        "manual",
        "prewrite",
        "probe",
        "idle",
        "retry",
        "write",
        "local",
        "remote",
        "current",
        "stale",
        "owned",
        "absent",
        "partial",
        "unknown",
        "usb",
        "macos",
        "windows",
        "logger",
        "read",
        "displays",
    ]
    .contains(&role)
    {
        return None;
    }
    let mut result = json!({"event":event,"role":role});
    for key in ["t_us", "utc_us", "duration_us"] {
        if let Some(value) = source.get(key).filter(|v| !v.is_null()) {
            result[key] = json!(value.as_u64()?);
        }
    }
    for (key, allowed) in [
        ("operation", &["get_input", "set_input", "get", "set"][..]),
        (
            "outcome",
            &[
                "spawned",
                "spawn_error",
                "wait_error",
                "output_timed_out",
                "output_error",
                "transport_ok",
                "transport_error",
                "exit_error",
                "timed_out",
                "prevalidation_error",
                "readback_ok",
                "readback_error",
            ][..],
        ),
        ("target", &["local", "remote", "current"][..]),
    ] {
        if let Some(value) = source.get(key).filter(|v| !v.is_null()) {
            let value = value.as_str()?;
            if !allowed.contains(&value) {
                return None;
            }
            result[key] = json!(value);
        } else if event == "ddc_command" {
            return None;
        }
    }
    if let Some(value) = source.get("exit_code").filter(|v| !v.is_null()) {
        let code = value.as_i64()?;
        result["exit_code"] = json!(i32::try_from(code).ok()?);
    }
    if event == "trace_dropped" {
        result["dropped_count"] = json!(source.get("value")?.as_u64()?);
    }
    Some(result)
}

fn sanitize_current(source: &Value) -> Value {
    let mut result = json!({"peer_runtime":"unknown"});
    for (key, allowed) in [
        ("mode", &["unverified", "single", "dual"][..]),
        (
            "verification",
            &[
                "not_checked",
                "historical_unknown",
                "single_observed",
                "single_inconclusive",
                "dual_observed",
                "needs_attention",
            ][..],
        ),
        ("local_os", &["macos", "windows", "linux", "unknown"][..]),
        ("remote_os", &["macos", "windows", "unknown"][..]),
        ("local_role", &["a", "b"][..]),
        (
            "effective_state",
            &[
                "paused",
                "editing",
                "sleeping",
                "unavailable",
                "waiting_usb",
                "attention",
                "active",
            ][..],
        ),
        ("ownership", &["unknown", "owned", "absent", "partial"][..]),
        (
            "display_result",
            &[
                "idle",
                "pending",
                "readback_confirmed",
                "sent_unverified",
                "failed",
            ][..],
        ),
    ] {
        result[key] = source
            .get(key)
            .and_then(Value::as_str)
            .filter(|s| allowed.contains(s))
            .map_or(Value::Null, |s| json!(s));
    }
    for key in [
        "automatic_requested",
        "lifecycle_ok",
        "sleeping",
        "editing",
        "enrolled",
        "qualification_current",
        "startup_blocked",
        "cleanup_warning",
    ] {
        result[key] = json!(source.get(key).and_then(Value::as_bool).unwrap_or(false));
    }
    for key in ["profile_count", "selected_hub_count"] {
        result[key] = json!(source.get(key).and_then(Value::as_u64).unwrap_or(0));
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        fs,
        path::PathBuf,
        sync::atomic::{AtomicU64, Ordering},
    };
    struct Fixture(PathBuf);
    impl Fixture {
        fn new() -> Self {
            static NEXT: AtomicU64 = AtomicU64::new(0);
            let path = std::env::temp_dir().join(format!(
                "portkey-diagnostics-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir_all(&path).unwrap();
            Self(path)
        }
        fn log(&self) -> PathBuf {
            self.0.join("trace.jsonl")
        }
        fn write(&self, suffix: &str, value: impl AsRef<[u8]>) {
            fs::write(self.0.join(format!("trace.jsonl{suffix}")), value).unwrap();
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    #[test]
    fn no_configured_history_sanitizes_summary_without_guessing_evidence() {
        let package = preview(
            json!({"mode":"/private/name","verification":"not_checked","peer_runtime":"online","names":["SECRET"],"status":"RAWERROR","profile_count":2,"display_result":"pending"}),
            None,
        );
        assert_eq!(package["history"]["availability"], "not_configured");
        assert_eq!(package["current"]["mode"], Value::Null);
        assert_eq!(package["current"]["peer_runtime"], "unknown");
        assert_eq!(package["current"]["profile_count"], 2);
        assert_eq!(package["current"]["display_result"], "pending");
        assert!(!package.to_string().contains("SECRET"));
        assert!(!package.to_string().contains("RAWERROR"));
    }
    #[test]
    fn rotates_in_order_and_only_exports_allowlisted_record_fields() {
        let fixture = Fixture::new();
        fixture.write(".5", "{\"event\":\"ownership_observed\",\"role\":\"owned\",\"t_us\":10,\"pid\":123,\"session\":456,\"display_id\":\"SECRET\",\"rawstderr\":\"SECRET\"}\n");
        fixture.write("", "{\"event\":\"ddc_command\",\"role\":\"macos\",\"t_us\":20,\"utc_us\":100,\"duration_us\":40,\"operation\":\"set_input\",\"outcome\":\"transport_ok\",\"target\":\"remote\",\"exit_code\":0,\"input\":7,\"path\":\"SECRET\"}\n");
        let package = preview(json!({}), Some(&fixture.log()));
        let history = &package["history"];
        assert_eq!(history["availability"], "available");
        assert_eq!(history["files_read"], 2);
        assert_eq!(history["files_missing"], 4);
        assert_eq!(history["records"][0]["t_us"], 10);
        assert_eq!(history["records"][1]["t_us"], 20);
        assert_eq!(history["records"][1]["outcome"], "transport_ok");
        assert_eq!(history["records"][1]["duration_us"], 40);
        for prohibited in [
            "SECRET",
            "display_id",
            "rawstderr",
            "\"pid\"",
            "\"session\"",
            "\"input\"",
            "\"path\"",
        ] {
            assert!(
                !package.to_string().contains(prohibited),
                "leaked {prohibited}"
            );
        }
    }
    #[test]
    fn malformed_categories_partial_and_oversize_lines_are_excluded() {
        let fixture = Fixture::new();
        let rows=format!("not json\n{{\"event\":\"SECRET\",\"role\":\"owned\"}}\n{}\n{{\"event\":\"ownership_observed\",\"role\":\"owned\",\"t_us\":3}}\npartial", "X".repeat(4097));
        fixture.write("", rows);
        let package = preview(json!({}), Some(&fixture.log()));
        let history = &package["history"];
        assert_eq!(history["records_rejected"], 3);
        assert_eq!(history["partial_records"], 1);
        assert_eq!(history["records"].as_array().unwrap().len(), 1);
        assert_eq!(history["records"][0]["t_us"], 3);
        assert!(history["truncated"].as_bool().unwrap());
        assert!(!package.to_string().contains("SECRET"));
    }
    #[test]
    fn caps_known_files_bytes_and_retained_records_while_keeping_recent_tail() {
        let fixture = Fixture::new();
        let rows=(0..2000).map(|n| format!("{{\"event\":\"inventory_end_ok\",\"role\":\"idle\",\"t_us\":{n},\"duration_us\":10}}\n")).collect::<String>();
        for suffix in ["", ".1", ".2", ".3", ".4", ".5"] {
            fixture.write(suffix, &rows);
        }
        fixture.write(".6", "SECRET\n");
        let package = preview(json!({}), Some(&fixture.log()));
        let history = &package["history"];
        assert_eq!(history["files_read"], 6);
        assert!(history["bytes_read"].as_u64().unwrap() <= 393216);
        assert_eq!(history["records"].as_array().unwrap().len(), 256);
        assert_eq!(history["records"][255]["t_us"], 1999);
        assert_eq!(history["truncated"], true);
        assert!(!package.to_string().contains("SECRET"));
    }
    #[test]
    fn missing_nonregular_and_invalid_paths_report_unavailable_without_paths() {
        let fixture = Fixture::new();
        let missing = preview(json!({}), Some(&fixture.log()));
        assert_eq!(missing["history"]["availability"], "unavailable");
        assert_eq!(missing["history"]["files_missing"], 6);
        fs::create_dir(fixture.log()).unwrap();
        let directory = preview(json!({}), Some(&fixture.log()));
        assert_eq!(directory["history"]["files_unavailable"], 1);
        let relative = preview(json!({}), Some(Path::new("SECRET")));
        assert_eq!(relative["history"]["availability"], "unavailable");
        assert!(!relative.to_string().contains("SECRET"));
    }
    #[test]
    fn untrusted_known_fields_cannot_smuggle_free_text_or_wrong_types() {
        let fixture = Fixture::new();
        let mut lines = String::new();
        for change in [
            json!({"role":"SECRET"}),
            json!({"operation":"SECRET"}),
            json!({"outcome":"SECRET"}),
            json!({"target":"SECRET"}),
            json!({"duration_us":"SECRET"}),
            json!({"exit_code":99999999999u64}),
        ] {
            let mut row = json!({"event":"ddc_command","role":"macos","operation":"set_input","outcome":"transport_ok","target":"local","t_us":1});
            for (key, value) in change.as_object().unwrap() {
                row[key] = value.clone();
            }
            lines.push_str(&row.to_string());
            lines.push('\n');
        }
        fixture.write("", lines);
        let package = preview(json!({}), Some(&fixture.log()));
        assert_eq!(package["history"]["records_rejected"], 6);
        assert_eq!(package["history"]["availability"], "empty");
        assert!(!package.to_string().contains("SECRET"));
    }
    #[cfg(unix)]
    #[test]
    fn symlink_history_is_not_followed() {
        let fixture = Fixture::new();
        let outside = fixture.0.join("private.jsonl");
        std::fs::write(&outside, "SECRET").unwrap();
        std::os::unix::fs::symlink(outside, fixture.log()).unwrap();
        let package = preview(json!({}), Some(&fixture.log()));
        assert_eq!(package["history"]["files_read"], 0);
        assert_eq!(package["history"]["files_unavailable"], 1);
        assert!(!package.to_string().contains("SECRET"));
    }
    #[test]
    fn dropped_records_are_reported_separately_from_visible_history() {
        let fixture = Fixture::new();
        fixture.write(
            ".1",
            "{\"event\":\"trace_dropped\",\"role\":\"logger\",\"t_us\":1,\"value\":3}\n",
        );
        fixture.write(
            "",
            "{\"event\":\"trace_dropped\",\"role\":\"logger\",\"t_us\":2,\"value\":7}\n",
        );
        let package = preview(json!({}), Some(&fixture.log()));
        assert_eq!(package["history"]["reported_dropped"], 10);
        assert_eq!(package["history"]["records"][1]["dropped_count"], 7);
    }
}
