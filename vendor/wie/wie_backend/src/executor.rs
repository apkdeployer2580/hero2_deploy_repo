use alloc::{boxed::Box, sync::Arc};
use core::{
    future::Future,
    pin::Pin,
    sync::atomic::{AtomicUsize, Ordering},
    task::{Context, Poll, RawWaker, RawWakerVTable, Waker},
};

use hashbrown::{HashMap, HashSet};
use spin::Mutex;

use wie_util::{Result, WieError};

use crate::time::Instant;

type Task = Pin<Box<dyn Future<Output = Result<()>> + Send>>;

static TASK_COUNT_MAX: AtomicUsize = AtomicUsize::new(0);

pub struct ExecutorInner {
    current_task_id: Option<usize>,
    tasks: HashMap<usize, Task>,
    sleeping_tasks: HashMap<usize, Instant>,
    last_task_id: usize,
    last_now: Instant,
    /// ★ 2026-10-21 (wie_app fork): the tasks that run GUEST code — the MIDlet's start task,
    /// every `Thread.start`, a WIPI-C timer. While `parked` they are not polled at all
    /// (their sleep entry stays as it is), so the executor keeps ticking the HOST tasks —
    /// the SMAF/MA-2 music player above all — with the game itself standing still. The
    /// hero2 world map: it holds the field the way a menu does, but a menu keeps the
    /// music going and a tick stop did not.
    parkable: HashSet<usize>,
    parked: bool,
}

impl ExecutorInner {
    /// `(how many tasks may be polled right now, the earliest wake-up among the sleeping
    /// ones that may)` — parked guest tasks count for neither.
    fn runnable(&self) -> (usize, Option<Instant>) {
        let mut running = 0usize;
        let mut next: Option<Instant> = None;
        for id in self.tasks.keys() {
            if self.parked && self.parkable.contains(id) {
                continue;
            }
            match self.sleeping_tasks.get(id) {
                Some(until) => next = Some(next.map_or(*until, |n| if *until < n { *until } else { n })),
                None => running += 1,
            }
        }
        (running, next)
    }
}

pub trait AsyncCallable<R>: Send
where
    R: Send,
{
    fn call(self) -> impl Future<Output = R> + Send;
}

impl<F, R, Fut> AsyncCallable<R> for F
where
    F: FnOnce() -> Fut + 'static + Send,
    R: AsyncCallableResult,
    Fut: Future<Output = R> + 'static + Send,
{
    async fn call(self) -> R {
        self().await
    }
}

pub trait AsyncCallableResult: Send {
    fn err(self) -> Option<WieError>;
}

impl<R> AsyncCallableResult for core::result::Result<R, WieError>
where
    R: Send,
{
    fn err(self) -> Option<WieError> {
        self.err()
    }
}

impl AsyncCallableResult for () {
    fn err(self) -> Option<WieError> {
        None
    }
}

#[derive(Clone)]
pub struct Executor {
    inner: Arc<Mutex<ExecutorInner>>,
}

impl Executor {
    #[allow(clippy::new_without_default)]
    pub fn new() -> Self {
        let inner = Arc::new(Mutex::new(ExecutorInner {
            current_task_id: None,
            tasks: HashMap::new(),
            sleeping_tasks: HashMap::new(),
            last_task_id: 0,
            last_now: Instant::from_epoch_millis(0),
            parkable: HashSet::new(),
            parked: false,
        }));

        Self { inner }
    }

    pub fn spawn<C, R>(&self, callable: C) -> usize
    where
        C: AsyncCallable<R> + 'static,
        R: AsyncCallableResult,
    {
        let fut = async move {
            let result = callable.call().await;
            if let Some(err) = result.err() {
                return Err(err);
            }

            Ok(())
        };

        let task_id = {
            let mut inner = self.inner.lock();
            inner.last_task_id += 1;
            inner.last_task_id
        };

        self.inner.lock().tasks.insert(task_id, Box::pin(fut));

        task_id
    }

    /// [`Self::spawn`] for a task that runs GUEST code — one [`Self::park`] stops.
    pub fn spawn_parkable<C, R>(&self, callable: C) -> usize
    where
        C: AsyncCallable<R> + 'static,
        R: AsyncCallableResult,
    {
        let task_id = self.spawn(callable);
        self.inner.lock().parkable.insert(task_id);
        task_id
    }

    /// Park (`true`) or release every parkable task. Parked tasks are neither polled nor
    /// counted as runnable, so a tick with only parked guest tasks and a sleeping music
    /// player sleeps out to the player's next event instead of spinning its 8 ms.
    pub fn park(&self, on: bool) {
        self.inner.lock().parked = on;
    }

    pub fn parked(&self) -> bool {
        self.inner.lock().parked
    }

    // TODO we need to remove error handling from here. we need to JoinHandle like on spawn..
    pub fn tick<T>(&mut self, now: T) -> Result<()>
    where
        T: Fn() -> Instant,
    {
        // ★ Task-count gauge (wie_app fork, 2026-08-06). Tasks are only removed on
        // `Poll::Ready`, so a spawned task that never completes stays forever — which
        // would show up here as a rising count and in RSS, separately from the poll
        // nesting the crash is about. Logs on a new maximum only.
        {
            let n = self.inner.lock().tasks.len();
            if n > TASK_COUNT_MAX.load(Ordering::Relaxed) {
                TASK_COUNT_MAX.store(n, Ordering::Relaxed);
                if n >= 8 {
                    tracing::warn!("executor task count {n}");
                }
            }
        }

        let end = now() + 8; // TODO hardcoded
        loop {
            let now = now();

            if now > end {
                break;
            }

            {
                let inner = self.inner.lock();
                let (running_task_count, next_wakeup) = inner.runnable();
                if running_task_count == 0 {
                    match next_wakeup {
                        Some(next) if now >= next => {}
                        // Nothing runnable and nothing due: every live task sleeps past
                        // `now`, or only parked tasks are left.
                        _ => break,
                    }
                }
            }

            self.step(now)?;
        }

        Ok(())
    }

    pub fn current_task_id(&self) -> u64 {
        self.inner.lock().current_task_id.unwrap() as _
    }

    /// Milliseconds until this executor has work again: `Some(0)` when a task is
    /// runnable right now, `Some(n)` when every task is asleep and the earliest wakes
    /// in `n` ms, `None` when there is nothing at all.
    ///
    /// ★ Exists so the HOST's worker loop can stop over-sleeping (2026-08-13,
    /// `MIGRATION_LGT.md` §2.24). `tick` already computes this to decide its own early
    /// break, but it used to keep the number to itself, so `wie_android` filled the
    /// rest of its 16 ms frame regardless — a guest waiting 3 ms for its next frame
    /// timer was not looked at again for 16.
    pub fn next_wakeup_in(&self, now: Instant) -> Option<u64> {
        let inner = self.inner.lock();
        if inner.tasks.is_empty() {
            return None;
        }
        let (running, next) = inner.runnable();
        if running > 0 {
            return Some(0);
        }
        let next = next?;
        Some(if next > now { next - now } else { 0 })
    }

    fn step(&mut self, now: Instant) -> Result<()> {
        self.inner.lock().last_now = now;

        let mut next_tasks = HashMap::new();
        let tasks = self.inner.lock().tasks.drain().collect::<HashMap<_, _>>();
        let mut sleeping_tasks = self.inner.lock().sleeping_tasks.drain().collect::<HashMap<_, _>>();

        let parked = self.inner.lock().parked;
        for (task_id, mut task) in tasks.into_iter() {
            if parked && self.inner.lock().parkable.contains(&task_id) {
                // Not polled, not woken: the task and its sleep entry (if any) stay as they are.
                next_tasks.insert(task_id, task);
                continue;
            }
            let item = sleeping_tasks.get(&task_id);
            if let Some(item) = item {
                if *item <= now {
                    sleeping_tasks.remove(&task_id);
                } else {
                    next_tasks.insert(task_id, task);
                    continue;
                }
            }

            let waker = self.create_waker();
            let mut context = Context::from_waker(&waker);
            self.inner.lock().current_task_id = Some(task_id);

            match task.as_mut().poll(&mut context) {
                Poll::Ready(x) => {
                    self.inner.lock().parkable.remove(&task_id);
                    x?;
                }
                Poll::Pending => {
                    next_tasks.insert(task_id, task);
                }
            }

            self.inner.lock().current_task_id = None;
        }

        self.inner.lock().sleeping_tasks.extend(sleeping_tasks);
        self.inner.lock().tasks.extend(next_tasks);

        Ok(())
    }

    pub(crate) fn sleep(&self, timeout: u64) {
        let task_id = self.inner.lock().current_task_id.unwrap();

        let until = self.inner.lock().last_now + timeout;
        self.inner.lock().sleeping_tasks.insert(task_id, until);
    }

    fn create_waker(&self) -> Waker {
        unsafe fn noop_clone(_data: *const ()) -> RawWaker {
            noop_raw_waker()
        }

        unsafe fn noop(_data: *const ()) {}

        const NOOP_WAKER_VTABLE: RawWakerVTable = RawWakerVTable::new(noop_clone, noop, noop, noop);

        const fn noop_raw_waker() -> RawWaker {
            RawWaker::new(core::ptr::null(), &NOOP_WAKER_VTABLE)
        }

        unsafe { Waker::from_raw(noop_raw_waker()) }
    }
}

#[cfg(test)]
mod park_tests {
    use alloc::sync::Arc;
    use core::sync::atomic::{AtomicUsize, Ordering};

    use super::Executor;
    use crate::{task::YieldFuture, time::Instant};

    /// Counts its polls and yields forever.
    async fn count_forever(c: Arc<AtomicUsize>) -> wie_util::Result<()> {
        loop {
            c.fetch_add(1, Ordering::Relaxed);
            YieldFuture::new().await;
        }
    }

    fn counter(executor: &Executor, parkable: bool) -> Arc<AtomicUsize> {
        let count = Arc::new(AtomicUsize::new(0));
        let c = count.clone();
        if parkable {
            executor.spawn_parkable(move || count_forever(c));
        } else {
            executor.spawn(move || count_forever(c));
        }
        count
    }

    /// One `step`: `tick` reads `now` once for its deadline and once per loop; the third
    /// reading is past the deadline, so exactly one step runs.
    fn tick_once(executor: &mut Executor, at: u64) {
        let t = Instant::from_epoch_millis(at);
        let n = core::cell::Cell::new(0u64);
        executor
            .tick(move || {
                n.set(n.get() + 1);
                if n.get() <= 2 { t } else { t + 100 }
            })
            .unwrap();
    }

    #[test]
    fn a_parked_guest_task_is_not_polled_while_a_host_task_still_is() {
        let mut executor = Executor::new();
        let guest = counter(&executor, true);
        let host = counter(&executor, false);
        tick_once(&mut executor, 0);
        assert_eq!((guest.load(Ordering::Relaxed), host.load(Ordering::Relaxed)), (1, 1));

        executor.park(true);
        tick_once(&mut executor, 10);
        tick_once(&mut executor, 20);
        assert_eq!(
            (guest.load(Ordering::Relaxed), host.load(Ordering::Relaxed)),
            (1, 3),
            "the guest stands, the host runs"
        );

        executor.park(false);
        tick_once(&mut executor, 30);
        assert_eq!(
            (guest.load(Ordering::Relaxed), host.load(Ordering::Relaxed)),
            (2, 4),
            "released: both run again"
        );
    }

    #[test]
    fn a_parked_guest_task_does_not_count_as_runnable() {
        let executor = Executor::new();
        let _guest = counter(&executor, true);
        assert_eq!(executor.next_wakeup_in(Instant::from_epoch_millis(0)), Some(0));
        executor.park(true);
        // Only a parked task is left: nothing is due and nothing will be — the host may sleep as it likes.
        assert_eq!(executor.next_wakeup_in(Instant::from_epoch_millis(0)), None);
        executor.park(false);
        assert_eq!(executor.next_wakeup_in(Instant::from_epoch_millis(0)), Some(0));
    }
}
