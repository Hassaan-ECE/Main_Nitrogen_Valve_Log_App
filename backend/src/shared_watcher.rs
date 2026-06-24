use std::{
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use notify::{Config, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use tauri::{AppHandle, Emitter, Runtime};

pub(crate) const VALVE_LOG_CHANGED_EVENT: &str = "valve-log:changed";
const WATCHER_EMIT_DEBOUNCE: Duration = Duration::from_millis(250);

pub(crate) struct SharedStateWatcher {
    state: Mutex<WatcherState>,
}

struct WatcherState {
    watched_path: Option<PathBuf>,
    watcher: Option<RecommendedWatcher>,
    last_emit: Arc<Mutex<Option<Instant>>>,
}

impl SharedStateWatcher {
    pub(crate) fn new() -> Self {
        Self {
            state: Mutex::new(WatcherState {
                watched_path: None,
                watcher: None,
                last_emit: Arc::new(Mutex::new(None)),
            }),
        }
    }

    pub(crate) fn ensure_watching<R: Runtime>(
        &self,
        app: AppHandle<R>,
        shared_dir: &Path,
    ) -> Result<(), String> {
        self.ensure_watching_with_emit(shared_dir, move || {
            let _ = app.emit(VALVE_LOG_CHANGED_EVENT, ());
        })
    }

    fn ensure_watching_with_emit<F>(&self, shared_dir: &Path, emit: F) -> Result<(), String>
    where
        F: Fn() + Send + 'static,
    {
        if !shared_dir.exists() {
            return Ok(());
        }

        let normalized_path = shared_dir.to_path_buf();
        let mut state = self
            .state
            .lock()
            .map_err(|_| "Shared state watcher is unavailable.".to_string())?;

        if state
            .watched_path
            .as_ref()
            .is_some_and(|current| current == &normalized_path)
        {
            return Ok(());
        }

        state.watcher.take();
        let last_emit = Arc::clone(&state.last_emit);
        let mut watcher = RecommendedWatcher::new(
            move |result: notify::Result<notify::Event>| {
                let Ok(event) = result else {
                    return;
                };
                if !event_kind_should_emit(&event.kind) {
                    return;
                }
                if should_debounce_emit(&last_emit) {
                    return;
                }
                emit();
            },
            Config::default(),
        )
        .map_err(|error| format!("Could not start shared state watcher: {error}"))?;

        watcher
            .watch(&normalized_path, RecursiveMode::Recursive)
            .map_err(|error| format!("Could not watch shared valve log folder: {error}"))?;

        state.watched_path = Some(normalized_path);
        state.watcher = Some(watcher);
        Ok(())
    }
}

fn event_kind_should_emit(kind: &EventKind) -> bool {
    matches!(
        kind,
        EventKind::Any | EventKind::Create(_) | EventKind::Modify(_) | EventKind::Remove(_)
    )
}

fn should_debounce_emit(last_emit: &Arc<Mutex<Option<Instant>>>) -> bool {
    let Ok(mut last_emit) = last_emit.lock() else {
        return true;
    };

    if last_emit
        .as_ref()
        .is_some_and(|instant| instant.elapsed() < WATCHER_EMIT_DEBOUNCE)
    {
        return true;
    }

    *last_emit = Some(Instant::now());
    false
}
