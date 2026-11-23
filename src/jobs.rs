//! Background jobs: run blocking system calls off the UI thread.
//!
//! Every server request and image download goes through [`Jobs::spawn`],
//! serviced by a bounded pool of worker threads so bursty browsing or typing
//! cannot spawn unbounded threads on low-RAM devices. The result is sent back
//! as a message and the UI is woken with `request_repaint`, so the screen
//! never freezes on a slow server.

use eframe::egui;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::thread;

type Task = Box<dyn FnOnce() + Send + 'static>;

pub struct Jobs<M: Send + 'static> {
    work_tx: Sender<Task>,
    tx: Sender<M>,
    rx: Receiver<M>,
    ctx: egui::Context,
}

impl<M: Send + 'static> Jobs<M> {
    pub fn new(ctx: &egui::Context, workers: usize) -> Self {
        let (tx, rx) = mpsc::channel();
        let (work_tx, work_rx) = mpsc::channel::<Task>();
        let shared = Arc::new(Mutex::new(work_rx));
        let count = workers.max(1);
        for i in 0..count {
            let rx = Arc::clone(&shared);
            if let Err(e) = thread::Builder::new()
                .name(format!("cattube-worker-{i}"))
                .spawn(move || {
                    loop {
                        let task = {
                            let guard = rx.lock().unwrap_or_else(|e| e.into_inner());
                            guard.recv()
                        };
                        match task {
                            Ok(job) => job(),
                            Err(_) => break,
                        }
                    }
                })
            {
                eprintln!("cattube: could not start worker thread {i}: {e}");
            }
        }
        Self {
            work_tx,
            tx,
            rx,
            ctx: ctx.clone(),
        }
    }

    /// Run `f` on the worker pool and deliver its message.
    pub fn spawn<F>(&self, f: F)
    where
        F: FnOnce() -> M + Send + 'static,
    {
        self.try_spawn(f);
    }

    /// Like [`Jobs::spawn`] but returns false (and logs) if the task could
    /// not be queued, for callers that track "in flight" state.
    pub fn try_spawn<F>(&self, f: F) -> bool
    where
        F: FnOnce() -> M + Send + 'static,
    {
        self.queue(f, None::<fn() -> M>)
    }

    /// Like [`Jobs::spawn`], for a job a screen is waiting on: if it panics,
    /// `on_panic`'s message is delivered instead, so no spinner stays forever.
    pub fn spawn_or<F, R>(&self, f: F, on_panic: R)
    where
        F: FnOnce() -> M + Send + 'static,
        R: FnOnce() -> M + Send + 'static,
    {
        self.queue(f, Some(on_panic));
    }

    /// [`Jobs::spawn_or`] for callers that track "in flight" state.
    pub fn try_spawn_or<F, R>(&self, f: F, on_panic: R) -> bool
    where
        F: FnOnce() -> M + Send + 'static,
        R: FnOnce() -> M + Send + 'static,
    {
        self.queue(f, Some(on_panic))
    }

    fn queue<F, R>(&self, f: F, on_panic: Option<R>) -> bool
    where
        F: FnOnce() -> M + Send + 'static,
        R: FnOnce() -> M + Send + 'static,
    {
        let tx = self.tx.clone();
        let ctx = self.ctx.clone();
        let job: Task = Box::new(move || {
            // A failed job must not take its worker thread with it.
            let msg = match catch_unwind(AssertUnwindSafe(f)) {
                Ok(msg) => Some(msg),
                Err(_) => {
                    eprintln!("cattube: a background job failed");
                    on_panic.map(|r| r())
                }
            };
            if let Some(msg) = msg {
                let _ = tx.send(msg);
                ctx.request_repaint();
            }
        });
        match self.work_tx.send(job) {
            Ok(()) => true,
            Err(e) => {
                eprintln!("cattube: could not queue background job: {e}");
                false
            }
        }
    }

    /// A sender + repaint handle for long-running producers (the playback
    /// reporter).
    pub fn sender(&self) -> (Sender<M>, egui::Context) {
        (self.tx.clone(), self.ctx.clone())
    }

    /// All messages received since the last call.
    pub fn drain(&self) -> Vec<M> {
        self.rx.try_iter().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::{Duration, Instant};

    #[test]
    fn pool_runs_all_tasks_within_worker_limit() {
        let ctx = egui::Context::default();
        let jobs: Jobs<usize> = Jobs::new(&ctx, 2);
        let active = Arc::new(AtomicUsize::new(0));
        let peak = Arc::new(AtomicUsize::new(0));
        for i in 0..10 {
            let active = Arc::clone(&active);
            let peak = Arc::clone(&peak);
            assert!(jobs.try_spawn(move || {
                let now = active.fetch_add(1, Ordering::SeqCst) + 1;
                peak.fetch_max(now, Ordering::SeqCst);
                thread::sleep(Duration::from_millis(10));
                active.fetch_sub(1, Ordering::SeqCst);
                i
            }));
        }
        let deadline = Instant::now() + Duration::from_secs(3);
        let mut got = Vec::new();
        while got.len() < 10 && Instant::now() < deadline {
            got.extend(jobs.drain());
            if got.len() < 10 {
                thread::sleep(Duration::from_millis(5));
            }
        }
        got.sort_unstable();
        assert_eq!(got, (0..10).collect::<Vec<_>>());
        assert!(peak.load(Ordering::SeqCst) <= 2);
    }

    #[test]
    fn a_panicking_job_keeps_the_worker_and_reports_its_failure() {
        let ctx = egui::Context::default();
        let jobs: Jobs<usize> = Jobs::new(&ctx, 1);
        jobs.spawn(|| panic!("expected in this test"));
        jobs.spawn_or(|| panic!("expected in this test"), || 1);
        assert!(jobs.try_spawn(|| 2));
        let deadline = Instant::now() + Duration::from_secs(3);
        let mut got = Vec::new();
        while got.len() < 2 && Instant::now() < deadline {
            got.extend(jobs.drain());
            thread::sleep(Duration::from_millis(5));
        }
        assert_eq!(got, [1, 2]);
    }
}
