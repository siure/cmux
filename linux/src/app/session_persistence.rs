//! Ordered session writes. Callers own the model and submit immutable snapshots.
use super::{write_session_snapshot, AppError, AppResult, LinuxSessionSnapshot};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

type Writer = dyn Fn(&Path, &LinuxSessionSnapshot) -> AppResult<usize> + Send + Sync;

pub(super) struct SessionPersistence {
    writer: Arc<Writer>,
    result: Mutex<AppResult<usize>>,
}

impl SessionPersistence {
    pub(super) fn new() -> AppResult<Self> {
        Self::with_writer(Arc::new(write_session_snapshot))
    }

    fn with_writer(writer: Arc<Writer>) -> AppResult<Self> {
        Ok(Self { writer, result: Mutex::new(Ok(0)) })
    }

    pub(super) fn enqueue(&self, path: PathBuf, snapshot: LinuxSessionSnapshot) {
        *self.result.lock().unwrap() = (self.writer)(&path, &snapshot);
    }

    pub(super) fn flush(&self) -> AppResult<usize> {
        self.result.lock().unwrap().clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;
    use std::time::Duration;

    fn snapshot(sequence: usize) -> LinuxSessionSnapshot {
        LinuxSessionSnapshot {
            version: super::super::SESSION_SNAPSHOT_VERSION,
            saved_at: sequence as f64,
            current_window_index: 0,
            windows: Vec::new(),
        }
    }

    #[test]
    fn slow_write_does_not_block_submission_and_pending_snapshots_coalesce() {
        let (started_tx, started_rx) = mpsc::channel();
        let (release_tx, release_rx) = mpsc::channel();
        let release_rx = Mutex::new(release_rx);
        let written = Arc::new(Mutex::new(Vec::new()));
        let observed = written.clone();
        let worker = Arc::new(SessionPersistence::with_writer(Arc::new(move |_, snapshot| {
            if snapshot.saved_at == 1.0 {
                started_tx.send(()).unwrap();
                release_rx.lock().unwrap().recv().unwrap();
            }
            observed.lock().unwrap().push(snapshot.saved_at as usize);
            Ok(snapshot.saved_at as usize)
        })).unwrap());
        let producer = worker.clone();
        let (submitted_tx, submitted_rx) = mpsc::channel();
        let submitter = std::thread::spawn(move || {
            producer.enqueue(PathBuf::from("session.json"), snapshot(1));
            submitted_tx.send(()).unwrap();
        });
        started_rx.recv_timeout(Duration::from_secs(2)).unwrap();
        let nonblocking = submitted_rx.recv_timeout(Duration::from_millis(250)).is_ok();
        if nonblocking {
            for sequence in 2..=50 {
                worker.enqueue(PathBuf::from("session.json"), snapshot(sequence));
            }
        }
        release_tx.send(()).unwrap();
        submitter.join().unwrap();
        worker.flush().unwrap();
        assert!(nonblocking, "snapshot submission blocked on the disk writer");
        assert_eq!(*written.lock().unwrap(), vec![1, 50]);
    }

    #[test]
    fn flush_reports_write_failure_and_later_snapshot_can_recover() {
        let worker = SessionPersistence::with_writer(Arc::new(|_, snapshot| {
            if snapshot.saved_at == 1.0 {
                Err(AppError::internal("disk full"))
            } else {
                Ok(42)
            }
        })).unwrap();
        worker.enqueue(PathBuf::from("session.json"), snapshot(1));
        assert_eq!(worker.flush().unwrap_err().message, "disk full");
        worker.enqueue(PathBuf::from("session.json"), snapshot(2));
        assert_eq!(worker.flush().unwrap(), 42);
    }

    #[test]
    fn drop_flushes_latest_snapshot_to_disk() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("session.json");
        let worker = SessionPersistence::new().unwrap();
        for sequence in 1..=50 {
            worker.enqueue(path.clone(), snapshot(sequence));
        }
        drop(worker);
        let saved = super::super::read_session_snapshot(&path).unwrap();
        assert_eq!(saved.saved_at, 50.0);
    }
}
