use std::{
    fs::{File, OpenOptions},
    path::Path,
};

/// The OS releases this advisory lock on exit, including crashes.
pub fn acquire(directory: &Path) -> Result<File, String> {
    std::fs::create_dir_all(directory).map_err(|e| e.to_string())?;
    let file = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(directory.join("instance.lock"))
        .map_err(|e| e.to_string())?;
    file.try_lock().map_err(|error| match error {
        std::fs::TryLockError::WouldBlock => {
            "PortKey is already running. Open its existing tray menu.".to_string()
        }
        std::fs::TryLockError::Error(error) => {
            format!("Cannot lock PortKey configuration: {error}")
        }
    })?;
    Ok(file)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn second_instance_is_rejected_and_exit_releases_lock() {
        let path = std::env::temp_dir().join(format!("portkey-instance-{}", std::process::id()));
        let first = acquire(&path).unwrap();
        assert!(acquire(&path).is_err());
        drop(first);
        // Concurrent process tests may briefly inherit the lock between fork and exec.
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(1);
        loop {
            if acquire(&path).is_ok() {
                break;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "OS did not release instance lock"
            );
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        std::fs::remove_dir_all(path).unwrap();
    }
}
