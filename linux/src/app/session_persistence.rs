//! Ordered session writes. Callers own the model and submit immutable snapshots.
use super::{write_session_snapshot, AppError, AppResult, LinuxSessionSnapshot};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Condvar, Mutex};
use std::thread::{self, JoinHandle};

type Writer = dyn Fn(&Path, &LinuxSessionSnapshot) -> AppResult<usize> + Send + Sync;

struct PendingSnapshot {
    generation: u64,
    path: PathBuf,
    snapshot: LinuxSessionSnapshot,
}

#[derive(Default)]
struct QueueState {
    submitted: u64,
    completed: u64,
    pending: Option<PendingSnapshot>,
    result: Option<AppResult<usize>>,
    stopping: bool,
}

#[derive(Default)]
struct Queue {
    state: Mutex<QueueState>,
    changed: Condvar,
}

pub(super) struct SessionPersistence {
    queue: Arc<Queue>,
    thread: Option<JoinHandle<()>>,
}

impl SessionPersistence {
    pub(super) fn new() -> AppResult<Self> {
        Self::with_writer(Arc::new(write_session_snapshot))
    }

    fn with_writer(writer: Arc<Writer>) -> AppResult<Self> {
        let queue = Arc::new(Queue::default());
        let worker_queue = queue.clone();
        let thread = thread::Builder::new()
            .name("cmux-session-save".into())
            .spawn(move || Self::run(worker_queue, writer))
            .map_err(|err| AppError::internal(format!("failed to start session writer: {err}")))?;
        Ok(Self {
            queue,
            thread: Some(thread),
        })
    }

    // AppState's model lock serializes producers. A full newer snapshot replaces
    // the waiting one; the running write always finishes before its successor.
    pub(super) fn enqueue(&self, path: PathBuf, snapshot: LinuxSessionSnapshot) {
        let mut state = self.queue.state.lock().unwrap();
        state.submitted += 1;
        state.pending = Some(PendingSnapshot {
            generation: state.submitted,
            path,
            snapshot,
        });
        self.queue.changed.notify_all();
    }

    // Explicit saves and shutdown call this while retaining exclusive access to
    // the model, so no producer can supersede the snapshot they just submitted.
    pub(super) fn flush(&self) -> AppResult<usize> {
        let mut state = self.queue.state.lock().unwrap();
        let target = state.submitted;
        while state.completed < target {
            state = self.queue.changed.wait(state).unwrap();
        }
        state.result.clone().unwrap_or(Ok(0))
    }

    fn run(queue: Arc<Queue>, writer: Arc<Writer>) {
        loop {
            let job = {
                let mut state = queue.state.lock().unwrap();
                loop {
                    if let Some(job) = state.pending.take() {
                        break job;
                    }
                    if state.stopping {
                        return;
                    }
                    state = queue.changed.wait(state).unwrap();
                }
            };
            // Serialization and filesystem I/O happen without the queue or model
            // lock. Catching a panic also ensures a waiting flush cannot hang.
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                writer(&job.path, &job.snapshot)
            }))
            .unwrap_or_else(|_| Err(AppError::internal("session writer panicked")));
            if let Err(error) = &result {
                eprintln!(
                    "session snapshot persist failed: {}: {}",
                    error.code, error.message
                );
            }
            let mut state = queue.state.lock().unwrap();
            state.completed = job.generation;
            state.result = Some(result);
            queue.changed.notify_all();
        }
    }
}

impl Drop for SessionPersistence {
    fn drop(&mut self) {
        {
            let mut state = self.queue.state.lock().unwrap();
            state.stopping = true;
            self.queue.changed.notify_all();
        }
        if let Some(thread) = self.thread.take() {
            if thread.join().is_err() {
                eprintln!("session writer failed during shutdown");
            }
        }
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
            closed_history: None,
        }
    }

    #[test]
    fn slow_write_does_not_block_submission_and_pending_snapshots_coalesce() {
        let (started_tx, started_rx) = mpsc::channel();
        let (release_tx, release_rx) = mpsc::channel();
        let release_rx = Mutex::new(release_rx);
        let written = Arc::new(Mutex::new(Vec::new()));
        let observed = written.clone();
        let worker = Arc::new(
            SessionPersistence::with_writer(Arc::new(move |_, snapshot| {
                if snapshot.saved_at == 1.0 {
                    started_tx.send(()).unwrap();
                    release_rx.lock().unwrap().recv().unwrap();
                }
                observed.lock().unwrap().push(snapshot.saved_at as usize);
                Ok(snapshot.saved_at as usize)
            }))
            .unwrap(),
        );
        let producer = worker.clone();
        let (submitted_tx, submitted_rx) = mpsc::channel();
        let submitter = std::thread::spawn(move || {
            producer.enqueue(PathBuf::from("session.json"), snapshot(1));
            submitted_tx.send(()).unwrap();
        });
        started_rx.recv_timeout(Duration::from_secs(2)).unwrap();
        let nonblocking = submitted_rx
            .recv_timeout(Duration::from_millis(250))
            .is_ok();
        if nonblocking {
            for sequence in 2..=50 {
                worker.enqueue(PathBuf::from("session.json"), snapshot(sequence));
            }
        }
        release_tx.send(()).unwrap();
        submitter.join().unwrap();
        worker.flush().unwrap();
        assert!(
            nonblocking,
            "snapshot submission blocked on the disk writer"
        );
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
        }))
        .unwrap();
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

    #[test]
    fn autosave_releases_model_lock_while_disk_writer_is_busy() {
        use super::super::{AppState, TerminalStartupMode};
        let (started_tx, started_rx) = mpsc::channel();
        let (release_tx, release_rx) = mpsc::channel();
        let release_rx = Mutex::new(release_rx);
        let worker = SessionPersistence::with_writer(Arc::new(move |_, _| {
            started_tx.send(()).unwrap();
            release_rx.lock().unwrap().recv().unwrap();
            Ok(1)
        }))
        .unwrap();
        let mut app = AppState::with_paths_and_terminal_startup(
            None,
            None,
            TerminalStartupMode::RendererOwned,
        )
        .unwrap();
        assert!(app.session_persistence.set(Ok(worker)).is_ok());
        app.async_session_autosave = true;
        let app = Arc::new(Mutex::new(app));
        let producer = app.clone();
        let (returned_tx, returned_rx) = mpsc::channel();
        let submitter = std::thread::spawn(move || {
            producer
                .lock()
                .unwrap()
                .autosave_session_snapshot()
                .unwrap();
            returned_tx.send(()).unwrap();
        });
        started_rx.recv_timeout(Duration::from_secs(2)).unwrap();
        let returned = returned_rx.recv_timeout(Duration::from_millis(250)).is_ok();
        let model_available = app.try_lock().is_ok();
        release_tx.send(()).unwrap();
        submitter.join().unwrap();
        app.lock()
            .unwrap()
            .session_persistence()
            .unwrap()
            .flush()
            .unwrap();
        assert!(returned, "autosave waited for the disk writer");
        assert!(model_available, "disk I/O retained the model lock");
    }

    #[test]
    fn flush_waits_for_pending_snapshot_after_the_running_write() {
        let (started_tx, started_rx) = mpsc::channel();
        let (release_tx, release_rx) = mpsc::channel();
        let release_rx = Mutex::new(release_rx);
        let worker = Arc::new(
            SessionPersistence::with_writer(Arc::new(move |_, value| {
                started_tx.send(value.saved_at as usize).unwrap();
                release_rx.lock().unwrap().recv().unwrap();
                Ok(value.saved_at as usize)
            }))
            .unwrap(),
        );
        worker.enqueue(PathBuf::from("session.json"), snapshot(1));
        assert_eq!(started_rx.recv_timeout(Duration::from_secs(2)).unwrap(), 1);
        worker.enqueue(PathBuf::from("session.json"), snapshot(2));
        let waiter = worker.clone();
        let (flushed_tx, flushed_rx) = mpsc::channel();
        let flush = std::thread::spawn(move || flushed_tx.send(waiter.flush()).unwrap());
        release_tx.send(()).unwrap();
        assert_eq!(started_rx.recv_timeout(Duration::from_secs(2)).unwrap(), 2);
        let returned_early = flushed_rx.recv_timeout(Duration::from_millis(100)).is_ok();
        release_tx.send(()).unwrap();
        flush.join().unwrap();
        assert!(
            !returned_early,
            "flush returned before the pending write completed"
        );
        assert_eq!(flushed_rx.recv().unwrap().unwrap(), 2);
    }

    #[test]
    fn scrollback_tail_retains_unicode_character_boundaries() {
        for text in ["", "abcdef", "aé終🙂z"] {
            for limit in 0..=8 {
                let expected = text
                    .chars()
                    .rev()
                    .take(limit)
                    .collect::<Vec<_>>()
                    .into_iter()
                    .rev()
                    .collect::<String>();
                assert_eq!(super::super::bounded_text_tail(text, limit), expected);
            }
        }
    }

    #[test]
    fn restored_renderer_scrollback_is_bounded_before_future_captures() {
        use super::super::{AppState, TerminalStartupMode, SESSION_SNAPSHOT_SCROLLBACK_CHAR_LIMIT};
        let mut app = AppState::with_paths_and_terminal_startup(
            None,
            None,
            TerminalStartupMode::RendererOwned,
        )
        .unwrap();
        let mut snapshot = app.session_snapshot(true);
        snapshot.windows[0].workspaces[0].panes[0].surfaces[0].scrollback =
            Some("終".repeat(SESSION_SNAPSHOT_SCROLLBACK_CHAR_LIMIT + 100));
        app.restore_session_snapshot(snapshot).unwrap();
        let captured = app.session_snapshot(true);
        assert_eq!(
            captured.windows[0].workspaces[0].panes[0].surfaces[0]
                .scrollback
                .as_ref()
                .unwrap()
                .chars()
                .count(),
            SESSION_SNAPSHOT_SCROLLBACK_CHAR_LIMIT
        );
    }

    #[test]
    #[ignore = "manual session capture and serialization timing"]
    fn profile_session_snapshot_capture() {
        use super::super::{AppState, TerminalStartupMode};
        use serde_json::json;
        let mut app = AppState::with_paths_and_terminal_startup(
            None,
            None,
            TerminalStartupMode::RendererOwned,
        )
        .unwrap();
        for _ in 1..24 {
            app.handle("surface.create", &json!({"type": "terminal"}))
                .unwrap();
        }
        let scrollback = "abcd終🙂\n".repeat(131_072);
        let surfaces = app.surfaces.keys().cloned().collect::<Vec<_>>();
        for surface in surfaces {
            app.update_embedded_terminal_scrollback_snapshot(&surface, &scrollback)
                .unwrap();
        }
        let started = std::time::Instant::now();
        let snapshot = app.session_snapshot(true);
        let capture = started.elapsed();
        let started = std::time::Instant::now();
        let bytes = serde_json::to_vec_pretty(&snapshot).unwrap();
        let serialize = started.elapsed();
        eprintln!(
            "session profile: {} surfaces, {} encoded bytes; capture {:?}, serialize {:?}",
            app.surfaces.len(),
            bytes.len(),
            capture,
            serialize
        );
        assert_eq!(
            snapshot.windows[0].workspaces[0].panes[0].surfaces.len(),
            24
        );
    }
}
