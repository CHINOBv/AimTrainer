//! Requests that run off the frame: a pure lifecycle ([`Request`]) and a background
//! thread behind it ([`Job`]). The UI polls a job once per frame and never waits on it.

use std::thread::JoinHandle;

/// Where a request is in its lifecycle.
#[derive(Debug, Clone, PartialEq)]
pub enum Phase<T> {
    /// Nothing asked yet, or the last outcome was taken.
    Idle,
    /// Waiting for the worker.
    Pending,
    /// The worker answered.
    Done(T),
    /// The worker failed; the message is for the player.
    Failed(String),
    /// The player cancelled; the worker's late answer is dropped.
    Cancelled,
}

/// The lifecycle of one request at a time: idle → pending → done, failed or cancelled.
/// Each start issues a ticket; an answer with an older ticket (a cancelled request that
/// finished late) is ignored.
#[derive(Debug, Clone, PartialEq)]
pub struct Request<T> {
    phase: Phase<T>,
    ticket: u64,
}

impl<T> Default for Request<T> {
    fn default() -> Self {
        Self {
            phase: Phase::Idle,
            ticket: 0,
        }
    }
}

impl<T> Request<T> {
    /// The current phase.
    pub fn phase(&self) -> &Phase<T> {
        &self.phase
    }

    /// Whether the worker is still running.
    pub fn is_pending(&self) -> bool {
        matches!(self.phase, Phase::Pending)
    }

    /// Start a request and return its ticket, or `None` while one is pending.
    pub fn start(&mut self) -> Option<u64> {
        if self.is_pending() {
            return None;
        }
        self.ticket += 1;
        self.phase = Phase::Pending;
        Some(self.ticket)
    }

    /// Record the worker's answer for `ticket`; returns whether it was accepted (the
    /// request is pending and the ticket is current).
    pub fn finish(&mut self, ticket: u64, outcome: Result<T, String>) -> bool {
        if !self.is_pending() || ticket != self.ticket {
            return false;
        }
        self.phase = match outcome {
            Ok(value) => Phase::Done(value),
            Err(message) => Phase::Failed(message),
        };
        true
    }

    /// Stop waiting: a pending request becomes cancelled. Other phases are unchanged.
    pub fn cancel(&mut self) {
        if self.is_pending() {
            self.phase = Phase::Cancelled;
        }
    }

    /// Take a finished outcome, leaving the request idle. Other phases return `None` and
    /// stay as they are (a cancelled request shows as cancelled until the next start).
    pub fn take(&mut self) -> Option<Result<T, String>> {
        match std::mem::replace(&mut self.phase, Phase::Idle) {
            Phase::Done(value) => Some(Ok(value)),
            Phase::Failed(message) => Some(Err(message)),
            other => {
                self.phase = other;
                None
            }
        }
    }
}

/// A [`Request`] whose work runs on a background thread. Cancelling detaches the thread:
/// its answer is dropped when it arrives (an HTTP request or a CLI process still runs to
/// its own timeout).
pub struct Job<T> {
    request: Request<T>,
    worker: Option<(u64, JoinHandle<Result<T, String>>)>,
}

impl<T> Default for Job<T> {
    fn default() -> Self {
        Self {
            request: Request::default(),
            worker: None,
        }
    }
}

impl<T: Send + 'static> Job<T> {
    /// The request's lifecycle.
    pub fn request(&self) -> &Request<T> {
        &self.request
    }

    /// Whether the worker is still running.
    pub fn is_pending(&self) -> bool {
        self.request.is_pending()
    }

    /// Run `work` on a new thread named `name`. Returns false while another run is
    /// pending; a thread that cannot start fails the request.
    pub fn spawn(
        &mut self,
        name: &str,
        work: impl FnOnce() -> Result<T, String> + Send + 'static,
    ) -> bool {
        let Some(ticket) = self.request.start() else {
            return false;
        };
        match std::thread::Builder::new()
            .name(name.to_owned())
            .spawn(work)
        {
            Ok(handle) => self.worker = Some((ticket, handle)),
            Err(e) => {
                self.request
                    .finish(ticket, Err(format!("could not start a worker thread: {e}")));
            }
        }
        true
    }

    /// Collect the worker's answer if it is ready. Never blocks.
    pub fn poll(&mut self) {
        if !self
            .worker
            .as_ref()
            .is_some_and(|(_, handle)| handle.is_finished())
        {
            return;
        }
        if let Some((ticket, handle)) = self.worker.take() {
            let outcome = handle
                .join()
                .unwrap_or_else(|_| Err("the worker stopped unexpectedly".to_owned()));
            self.request.finish(ticket, outcome);
        }
    }

    /// Stop waiting for the worker and drop its answer.
    pub fn cancel(&mut self) {
        self.request.cancel();
        self.worker = None;
    }

    /// Take a finished outcome (see [`Request::take`]).
    pub fn take(&mut self) -> Option<Result<T, String>> {
        self.request.take()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_request_goes_from_idle_to_pending_to_done_and_back_to_idle() {
        let mut r = Request::<u32>::default();
        assert_eq!(r.phase(), &Phase::Idle);
        let ticket = r.start().expect("an idle request starts");
        assert_eq!(r.phase(), &Phase::Pending);
        assert_eq!(r.start(), None, "one request at a time");
        assert!(r.finish(ticket, Ok(7)));
        assert_eq!(r.phase(), &Phase::Done(7));
        assert_eq!(r.take(), Some(Ok(7)));
        assert_eq!(r.phase(), &Phase::Idle);
        assert_eq!(r.take(), None);
    }

    #[test]
    fn a_failure_carries_its_message() {
        let mut r = Request::<u32>::default();
        let ticket = r.start().unwrap();
        assert!(r.finish(ticket, Err("rate limit".into())));
        assert_eq!(r.phase(), &Phase::Failed("rate limit".into()));
        assert_eq!(r.take(), Some(Err("rate limit".into())));
        assert_eq!(r.phase(), &Phase::Idle);
    }

    #[test]
    fn a_cancelled_request_drops_its_late_answer_and_can_start_again() {
        let mut r = Request::<u32>::default();
        let old = r.start().unwrap();
        r.cancel();
        assert_eq!(r.phase(), &Phase::Cancelled);
        assert!(!r.finish(old, Ok(1)), "the late answer is dropped");
        assert_eq!(r.take(), None);
        assert_eq!(r.phase(), &Phase::Cancelled, "shown until the next start");

        let new = r.start().expect("a cancelled request starts again");
        assert!(!r.finish(old, Ok(1)), "an old ticket never lands");
        assert!(r.finish(new, Ok(2)));
        assert_eq!(r.take(), Some(Ok(2)));
    }

    #[test]
    fn cancel_and_finish_do_nothing_outside_pending() {
        let mut r = Request::<u32>::default();
        r.cancel();
        assert_eq!(r.phase(), &Phase::Idle);
        assert!(!r.finish(0, Ok(1)));
        assert_eq!(r.phase(), &Phase::Idle);
        let t = r.start().unwrap();
        r.finish(t, Ok(3));
        r.cancel();
        assert_eq!(r.phase(), &Phase::Done(3), "a finished answer is kept");
    }

    #[test]
    fn a_job_runs_off_the_calling_thread_and_is_collected_by_polling() {
        let mut job = Job::<String>::default();
        let caller = std::thread::current().id();
        assert!(job.spawn("test-job", move || {
            Ok(format!("{}", std::thread::current().id() != caller))
        }));
        assert!(
            !job.spawn("test-job", || Ok(String::new())),
            "one at a time"
        );
        let start = std::time::Instant::now();
        while job.is_pending() && start.elapsed().as_secs() < 10 {
            job.poll();
            std::thread::yield_now();
        }
        assert_eq!(job.take(), Some(Ok("true".into())));
    }

    #[test]
    fn a_cancelled_job_ignores_its_worker_and_a_panic_is_a_failure() {
        let mut job = Job::<u32>::default();
        job.spawn("test-job", || Ok(1));
        job.cancel();
        job.poll();
        assert_eq!(job.request().phase(), &Phase::Cancelled);

        let mut job = Job::<u32>::default();
        job.spawn("test-job", || panic!("boom"));
        let start = std::time::Instant::now();
        while job.is_pending() && start.elapsed().as_secs() < 10 {
            job.poll();
            std::thread::yield_now();
        }
        assert_eq!(
            job.take(),
            Some(Err("the worker stopped unexpectedly".into()))
        );
    }
}
