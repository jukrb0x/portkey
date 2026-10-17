//! Explicit, bounded local latency diagnostics. Never pass device data or raw errors.
use std::{
    fs::{self, File, OpenOptions},
    io::{self, Read, Seek, SeekFrom, Write},
    path::PathBuf,
    sync::{
        atomic::{AtomicU64, Ordering},
        mpsc::{sync_channel, SyncSender},
        Arc, OnceLock,
    },
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

const LIMIT: u64 = 1024 * 1024;
const BACKUPS: usize = 5;
struct Trace {
    started: Instant,
    session: u64,
    sender: SyncSender<String>,
    dropped: Arc<AtomicU64>,
}
static TRACE: OnceLock<Option<Trace>> = OnceLock::new();
fn trace() -> Option<&'static Trace> {
    TRACE.get_or_init(|| {
        let path = PathBuf::from(std::env::var_os("PORTKEY_TIMING_LOG")?);
        if !path.is_absolute() { return None; }
        let (sender, receiver) = sync_channel::<String>(256);
        let started = Instant::now();
        let session = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_nanos() as u64;
        let dropped = Arc::new(AtomicU64::new(0));
        let writer_dropped = dropped.clone();
        std::thread::Builder::new().name("portkey-timing".into()).spawn(move || {
            let mut output = RotatingFile::new(path, LIMIT);
            let mut warned = false;
            for line in receiver {
                if !write_record(&mut output, &line, &writer_dropped, started, session) {
                    if !warned { eprintln!("PortKey timing log write failed; dropped records will be counted when logging recovers."); warned = true; }
                } else { warned = false; }
            }
        }).ok()?;
        Some(Trace { started, session, sender, dropped })
    }).as_ref()
}
pub fn enabled() -> bool {
    trace().is_some()
}
/// Labels must be static categories; value is a counter/generation, never an identity.
pub fn event(
    event: &'static str,
    role: &'static str,
    duration: Option<Duration>,
    value: Option<u64>,
) {
    let Some(trace) = trace() else {
        return;
    };
    let line = format_event(
        trace.started.elapsed(),
        std::process::id(),
        trace.session,
        event,
        role,
        duration,
        value,
    );
    // Diagnostics cannot block hardware work; account for a full or unavailable writer queue.
    enqueue(&trace.sender, line, &trace.dropped);
}
/// Structured DDC metadata only; never pass identifiers, argv or raw helper output.
pub struct CommandEvent {
    pub invocation: u64,
    pub operation: &'static str,
    pub outcome: &'static str,
    pub input: Option<u8>,
    pub target: &'static str,
    pub exit_code: Option<i32>,
    pub duration: Option<Duration>,
}
pub fn command_event(record: CommandEvent) {
    let Some(trace) = trace() else {
        return;
    };
    enqueue(
        &trace.sender,
        format_command(
            trace.started.elapsed(),
            std::process::id(),
            trace.session,
            &record,
        ),
        &trace.dropped,
    );
}
fn utc_micros() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_micros()
        .min(u64::MAX as u128) as u64
}
fn format_command(elapsed: Duration, process: u32, session: u64, record: &CommandEvent) -> String {
    let micros = |time: Duration| time.as_micros().min(u64::MAX as u128) as u64;
    let mut line = serde_json::json!({
        "t_us": micros(elapsed), "utc_us": utc_micros(), "pid": process, "session": session,
        "event": "ddc_command", "role": "macos", "value": record.invocation,
        "duration_us": record.duration.map(micros), "operation": record.operation,
        "outcome": record.outcome, "input": record.input, "target": record.target,
        "exit_code": record.exit_code
    })
    .to_string();
    line.push('\n');
    line
}
fn enqueue(sender: &SyncSender<String>, line: String, dropped: &AtomicU64) {
    if sender.try_send(line).is_err() {
        dropped.fetch_add(1, Ordering::Relaxed);
    }
}
fn write_record(
    output: &mut RotatingFile,
    line: &str,
    dropped: &AtomicU64,
    started: Instant,
    session: u64,
) -> bool {
    let lost = dropped.swap(0, Ordering::Relaxed);
    let recovery = if lost > 0 {
        output.append(&format_event(
            started.elapsed(),
            std::process::id(),
            session,
            "trace_dropped",
            "logger",
            None,
            Some(lost),
        ))
    } else {
        Ok(())
    };
    if recovery.is_err() {
        dropped.fetch_add(lost, Ordering::Relaxed);
    }
    if recovery.and_then(|_| output.append(line)).is_err() {
        dropped.fetch_add(1, Ordering::Relaxed);
        false
    } else {
        true
    }
}
fn format_event(
    elapsed: Duration,
    process: u32,
    session: u64,
    event: &str,
    role: &str,
    duration: Option<Duration>,
    value: Option<u64>,
) -> String {
    let micros = |time: Duration| time.as_micros().min(u64::MAX as u128) as u64;
    let mut line = serde_json::json!({"t_us":micros(elapsed),"utc_us":utc_micros(),"pid":process,"session":session,"event":event,"role":role,"duration_us":duration.map(micros),"value":value}).to_string();
    line.push('\n');
    line
}

struct RotatingFile {
    path: PathBuf,
    file: Option<File>,
    family_lock: Option<File>,
    bytes: u64,
    limit: u64,
    rotation_step: Option<usize>,
    #[cfg(test)]
    fail_rotation_step: Option<usize>,
}
impl RotatingFile {
    fn new(path: PathBuf, limit: u64) -> Self {
        Self {
            path,
            file: None,
            family_lock: None,
            bytes: 0,
            limit,
            rotation_step: None,
            #[cfg(test)]
            fail_rotation_step: None,
        }
    }
    fn backup(&self, index: usize) -> PathBuf {
        let mut name = self.path.as_os_str().to_os_string();
        name.push(format!(".{index}"));
        PathBuf::from(name)
    }
    fn open(&mut self) -> io::Result<()> {
        if self.family_lock.is_none() {
            let mut path = self.path.as_os_str().to_os_string();
            path.push(".lock");
            let mut options = OpenOptions::new();
            options.create(true).truncate(false).read(true).write(true);
            #[cfg(unix)]
            {
                use std::os::unix::fs::OpenOptionsExt;
                options.mode(0o600);
            }
            let lock = options.open(PathBuf::from(path))?;
            lock.try_lock().map_err(|error| match error {
                fs::TryLockError::WouldBlock => io::Error::new(
                    io::ErrorKind::WouldBlock,
                    "Timing log family is already in use",
                ),
                fs::TryLockError::Error(error) => error,
            })?;
            // Keep the stable lock file: unlinking it would let another writer lock a new inode.
            self.family_lock = Some(lock);
        }
        if self.rotation_step.is_some() {
            return self.rotate();
        }
        if self.file.is_some() {
            return Ok(());
        }
        let mut options = OpenOptions::new();
        options.create(true).read(true).append(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options.open(&self.path)?;
        self.bytes = file.metadata()?.len();
        let partial = if self.bytes > 0 {
            file.seek(SeekFrom::End(-1))?;
            let mut tail = [0];
            file.read_exact(&mut tail)?;
            tail[0] != b'\n'
        } else {
            false
        };
        self.file = Some(file);
        if self.bytes >= self.limit || partial {
            self.rotate()?;
        }
        Ok(())
    }
    fn rotate(&mut self) -> io::Result<()> {
        // Single writer per path. Close before Windows rename; retry only the failed step.
        drop(self.file.take());
        if self.rotation_step.is_none() {
            for index in 1..=BACKUPS {
                match fs::symlink_metadata(self.backup(index)) {
                    Ok(meta) if !meta.is_file() => {
                        return Err(io::Error::other("Timing archive is not a regular file"))
                    }
                    Err(error) if error.kind() != io::ErrorKind::NotFound => return Err(error),
                    _ => {}
                }
            }
            self.rotation_step = Some(BACKUPS);
        }
        while let Some(step) = self.rotation_step {
            #[cfg(test)]
            if self.fail_rotation_step == Some(step) {
                self.fail_rotation_step = None;
                return Err(io::Error::other("injected rename failure"));
            }
            let result = if step == BACKUPS {
                fs::remove_file(self.backup(BACKUPS))
            } else if step == 0 {
                fs::rename(&self.path, self.backup(1))
            } else {
                fs::rename(self.backup(step), self.backup(step + 1))
            };
            match result {
                Err(error) if error.kind() != io::ErrorKind::NotFound || step == 0 => {
                    return Err(error)
                }
                _ => {}
            }
            self.rotation_step = step.checked_sub(1);
        }
        self.bytes = 0;
        self.open()
    }
    fn append(&mut self, line: &str) -> io::Result<()> {
        if line.len() as u64 > self.limit
            || !line.ends_with('\n')
            || line[..line.len() - 1].contains('\n')
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Timing record exceeds limit or is not one complete line",
            ));
        }
        self.open()?;
        if self.bytes.saturating_add(line.len() as u64) > self.limit {
            self.rotate()?;
        }
        let result = self
            .file
            .as_mut()
            .ok_or_else(|| io::Error::other("Timing file unavailable"))?
            .write_all(line.as_bytes());
        if result.is_err() {
            // Reopen on the next record; any incomplete write is isolated in an archive.
            drop(self.file.take());
        } else {
            self.bytes += line.len() as u64;
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    struct Temp {
        directory: PathBuf,
        path: PathBuf,
    }
    impl Temp {
        fn new() -> Self {
            static NEXT: AtomicU64 = AtomicU64::new(0);
            let directory = std::env::temp_dir().join(format!(
                "portkey-rotation-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir_all(&directory).unwrap();
            Self {
                path: directory.join("timing.jsonl"),
                directory,
            }
        }
    }
    impl Drop for Temp {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.directory);
        }
    }
    fn contents(path: &std::path::Path) -> String {
        fs::read_to_string(path).unwrap()
    }
    #[test]
    fn record_format_has_monotonic_units_and_no_payload() {
        let line = format_event(
            Duration::from_micros(123),
            7,
            42,
            "inventory_end",
            "idle",
            Some(Duration::from_millis(4)),
            Some(9),
        );
        let mut record: serde_json::Value = serde_json::from_str(&line).unwrap();
        assert!(record["utc_us"].as_u64().unwrap() > 0);
        record.as_object_mut().unwrap().remove("utc_us");
        assert_eq!(
            record,
            serde_json::json!({"t_us":123,"pid":7,"session":42,"event":"inventory_end","role":"idle","duration_us":4000,"value":9})
        );
        assert!(line.ends_with('\n'));
    }
    #[test]
    fn command_record_has_numeric_input_result_and_utc_without_device_identity() {
        let detail = CommandEvent {
            invocation: 9,
            operation: "set",
            outcome: "transport_ok",
            input: Some(5),
            target: "local",
            exit_code: Some(0),
            duration: Some(Duration::from_millis(40)),
        };
        let value: serde_json::Value =
            serde_json::from_str(&format_command(Duration::from_micros(100), 7, 42, &detail))
                .unwrap();
        assert_eq!(value["event"], "ddc_command");
        assert_eq!(value["value"], 9);
        assert_eq!(value["input"], 5);
        assert_eq!(value["operation"], "set");
        assert_eq!(value["outcome"], "transport_ok");
        assert_eq!(value["exit_code"], 0);
        assert_eq!(value["duration_us"], 40000);
        assert!(value["utc_us"].as_u64().unwrap() > 0);
        assert!(value.get("display_id").is_none());
        assert!(value.get("argv").is_none());
    }
    #[test]
    fn full_log_must_continue_accepting_records() {
        let temp = Temp::new();
        let mut log = RotatingFile::new(temp.path.clone(), 4);
        log.append("123\n").unwrap();
        log.append("4\n").unwrap();
        assert_eq!(contents(&temp.path), "4\n");
        assert_eq!(contents(&log.backup(1)), "123\n");
    }
    #[test]
    fn startup_full_rotates_and_partial_tail_is_preserved_separately() {
        let temp = Temp::new();
        fs::write(&temp.path, "123\n").unwrap();
        let mut log = RotatingFile::new(temp.path.clone(), 4);
        log.append("4\n").unwrap();
        drop(log);
        fs::write(&temp.path, "5\n6").unwrap();
        let mut restarted = RotatingFile::new(temp.path.clone(), 4);
        restarted.append("7\n").unwrap();
        assert_eq!(contents(&temp.path), "7\n");
        assert_eq!(contents(&restarted.backup(1)), "5\n6");
        assert_eq!(contents(&restarted.backup(2)), "123\n");
    }
    #[test]
    fn multiple_rotations_bound_retention_and_keep_whole_json_records() {
        let temp = Temp::new();
        let mut log = RotatingFile::new(temp.path.clone(), 8);
        for n in 0..10 {
            log.append(&format!("{{\"n\":{n}}}\n")).unwrap();
        }
        assert_eq!(contents(&temp.path), "{\"n\":9}\n");
        for index in 1..=BACKUPS {
            let bytes = contents(&log.backup(index));
            assert_eq!(bytes, format!("{{\"n\":{}}}\n", 9 - index));
            assert!(serde_json::from_str::<serde_json::Value>(&bytes).is_ok());
            assert!(bytes.len() <= 8);
        }
        assert!(!log.backup(BACKUPS + 1).exists());
        assert_eq!(fs::read_dir(&temp.directory).unwrap().count(), 7);
    }
    #[test]
    fn restart_appends_without_rotation_when_complete_records_fit() {
        let temp = Temp::new();
        {
            let mut log = RotatingFile::new(temp.path.clone(), 8);
            log.append("1\n").unwrap();
        }
        let mut log = RotatingFile::new(temp.path.clone(), 8);
        log.append("2\n").unwrap();
        assert_eq!(contents(&temp.path), "1\n2\n");
        assert!(!log.backup(1).exists());
    }
    #[test]
    fn oversized_and_partial_records_do_not_damage_current_log() {
        let temp = Temp::new();
        let mut log = RotatingFile::new(temp.path.clone(), 4);
        log.append("1\n").unwrap();
        for line in ["12345\n", "", "partial", "1\n2\n"] {
            assert!(log.append(line).is_err());
        }
        assert_eq!(contents(&temp.path), "1\n");
        assert!(!log.backup(1).exists());
        log.append("2\n").unwrap();
        assert_eq!(contents(&temp.path), "1\n2\n");
    }
    #[test]
    fn archive_failure_does_not_panic_and_recovery_retries_rotation() {
        let temp = Temp::new();
        let mut log = RotatingFile::new(temp.path.clone(), 4);
        log.append("123\n").unwrap();
        fs::create_dir(log.backup(1)).unwrap();
        assert!(log.append("4\n").is_err());
        assert_eq!(contents(&temp.path), "123\n");
        fs::remove_dir(log.backup(1)).unwrap();
        log.append("4\n").unwrap();
        assert_eq!(contents(&temp.path), "4\n");
        assert_eq!(contents(&log.backup(1)), "123\n");
    }
    #[test]
    fn full_or_disconnected_queue_counts_dropped_records() {
        let (sender, receiver) = sync_channel(1);
        let dropped = AtomicU64::new(0);
        enqueue(&sender, "first\n".into(), &dropped);
        enqueue(&sender, "second\n".into(), &dropped);
        assert_eq!(dropped.load(Ordering::Relaxed), 1);
        assert_eq!(receiver.recv().unwrap(), "first\n");
        drop(receiver);
        enqueue(&sender, "third\n".into(), &dropped);
        assert_eq!(dropped.load(Ordering::Relaxed), 2);
    }
    #[test]
    fn failed_drop_marker_retains_count_until_logging_recovers() {
        let temp = Temp::new();
        let mut log = RotatingFile::new(temp.path.clone(), 512);
        fs::write(&temp.path, format!("{}\n", "x".repeat(511))).unwrap();
        fs::create_dir(log.backup(1)).unwrap();
        let dropped = AtomicU64::new(3);
        let started = Instant::now();
        assert!(!write_record(&mut log, "{}\n", &dropped, started, 42));
        assert_eq!(dropped.load(Ordering::Relaxed), 4);
        fs::remove_dir(log.backup(1)).unwrap();
        assert!(write_record(&mut log, "{}\n", &dropped, started, 42));
        assert_eq!(dropped.load(Ordering::Relaxed), 0);
        let text = contents(&temp.path);
        let rows: Vec<serde_json::Value> = text
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect();
        assert_eq!(rows[0]["event"], "trace_dropped");
        assert_eq!(rows[0]["value"], 4);
        assert_eq!(rows[1], serde_json::json!({}));
    }
    #[test]
    fn mid_rotation_failure_resumes_without_repeated_archive_aging() {
        let temp = Temp::new();
        let mut log = RotatingFile::new(temp.path.clone(), 4);
        fs::write(&temp.path, "old\n").unwrap();
        for n in 1..=BACKUPS {
            fs::write(log.backup(n), format!("{n}\n")).unwrap();
        }
        for _ in 0..2 {
            log.fail_rotation_step = Some(2);
            assert!(log.append("new\n").is_err());
            assert_eq!(contents(&log.backup(5)), "4\n");
            assert_eq!(contents(&log.backup(4)), "3\n");
            assert_eq!(contents(&temp.path), "old\n");
        }
        log.append("new\n").unwrap();
        assert_eq!(contents(&temp.path), "new\n");
        assert_eq!(contents(&log.backup(1)), "old\n");
        for n in 2..=BACKUPS {
            assert_eq!(contents(&log.backup(n)), format!("{}\n", n - 1));
        }
    }
    #[test]
    fn second_writer_cannot_rotate_same_family_and_recovers_after_release() {
        let temp = Temp::new();
        let mut first = RotatingFile::new(temp.path.clone(), 4);
        first.append("old\n").unwrap();
        let mut second = RotatingFile::new(temp.path.clone(), 4);
        assert!(
            second.append("new\n").is_err(),
            "another writer must not rotate the active family"
        );
        assert_eq!(contents(&temp.path), "old\n");
        assert!(!first.backup(1).exists());
        drop(first);
        let deadline = Instant::now() + Duration::from_secs(1);
        loop {
            if second.append("new\n").is_ok() {
                break;
            }
            assert!(
                Instant::now() < deadline,
                "OS did not release timing family lock"
            );
            std::thread::sleep(Duration::from_millis(5));
        }
        assert_eq!(contents(&temp.path), "new\n");
        assert_eq!(contents(&second.backup(1)), "old\n");
    }
}
