//! Async tasks: plugin logic that spans many pulses, written top to bottom.
//!
//! ```ignore
//! mq.spawn_fallible_task(|task: Task<MyPlugin>| async move {
//!     task.with(|mq| mq.face_point(&destination))?;
//!     task.wait_until(|mq| !mq.is_turning()).await;
//!     task.with(|mq| mq.set_key(MoveKey::Forward, true))?;
//!     task.delay(Duration::from_secs(2)).await;
//!     task.with(|mq| mq.stop_moving())
//! });
//! ```
//!
//! The runtime polls every task once per pulse, right after `on_pulse`, on the
//! game thread; there are no threads and no async runtime. A task never holds
//! an [`Mq`] across an `.await`: game objects are only valid within a pulse, so
//! the task reaches the game through [`Task::with`], whose closure nothing
//! borrowed from the game can escape.
//!
//! [`Mq::spawn_task`] takes tasks that return `()` (including endless loops);
//! [`Mq::spawn_fallible_task`] takes tasks returning `Result<(), E>`, so `?`
//! works, and reports an error in chat.

use std::any::Any;
use std::cell::{Cell, RefCell};
use std::collections::BTreeSet;
use std::fmt;
use std::future::Future;
use std::marker::PhantomData;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::pin::Pin;
use std::task::{Context, Poll, Waker};
use std::time::Duration;

use crate::mq::Mq;
use crate::plugin::Plugin;
use crate::runtime::Runtime;

/// Identifies a spawned task, e.g. to cancel it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TaskId(u64);

impl fmt::Display for TaskId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}


/// A task's handle to the game and its plugin. Cheap to clone; can't leave
/// the game thread.
pub struct Task<P: Plugin> {
    runtime: &'static Runtime<P>,
    id: TaskId,
    _not_send: PhantomData<*const ()>,
}

impl<P: Plugin> Clone for Task<P> {
    fn clone(&self) -> Self {
        Task { runtime: self.runtime, id: self.id, _not_send: PhantomData }
    }
}

impl<P: Plugin> Task<P> {
    /// This task's ID (e.g. to cancel itself with `mq.cancel_task`).
    pub fn id(&self) -> TaskId {
        self.id
    }

    /// Runs `f` with access to the game. Nothing borrowed from the game (a
    /// `Spawn`, say) can be returned, since it's only valid during this pulse;
    /// return IDs, positions and other plain values instead.
    pub fn with<R>(&self, f: impl FnOnce(&Mq<P>) -> R) -> R {
        f(&Mq::new(self.runtime))
    }

    /// Runs `f` with the plugin's state as well.
    ///
    /// # Panics
    /// If the plugin is already borrowed, i.e. this is called from inside a
    /// plugin callback rather than from a task.
    pub fn with_plugin<R>(&self, f: impl FnOnce(&mut P, &Mq<P>) -> R) -> R {
        self.runtime.with_plugin(f)
    }

    /// Waits for the next pulse.
    pub fn next_pulse(&self) -> NextPulse {
        NextPulse { waited: false }
    }

    /// Waits at least `duration` (checked once per pulse, so it ends on the
    /// first pulse after the time has passed).
    pub async fn delay(&self, duration: Duration) {
        let now = self.runtime.host().now;
        let deadline = now() + duration;
        while now() < deadline {
            self.next_pulse().await;
        }
    }

    /// Waits until `condition` holds, checking it now and then once per pulse.
    pub async fn wait_until(&self, mut condition: impl FnMut(&Mq<P>) -> bool) {
        while !self.with(&mut condition) {
            self.next_pulse().await;
        }
    }

    /// Like [`Task::wait_until`], giving up after `timeout`. Returns whether
    /// the condition held.
    pub async fn wait_until_timeout(&self, mut condition: impl FnMut(&Mq<P>) -> bool, timeout: Duration) -> bool {
        let now = self.runtime.host().now;
        let deadline = now() + timeout;
        loop {
            if self.with(&mut condition) {
                return true;
            }
            if now() >= deadline {
                return false;
            }
            self.next_pulse().await;
        }
    }
}

/// Completes on the pulse after the one it was first awaited in.
#[must_use = "futures do nothing unless awaited"]
pub struct NextPulse {
    waited: bool,
}

impl Future for NextPulse {
    type Output = ();

    fn poll(mut self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<()> {
        if self.waited {
            Poll::Ready(())
        } else {
            self.waited = true;
            Poll::Pending
        }
    }
}

type BoxedTask = Pin<Box<dyn Future<Output = Option<String>>>>;

struct Slot {
    id: u64,
    future: BoxedTask,
}

/// Runs the tasks. Every task is polled once per pulse, so no wakers are
/// needed: a pending future is simply polled again next time.
pub(crate) struct Executor {
    /// Tasks waiting for the next round.
    tasks: RefCell<Vec<Slot>>,
    /// Spawned since the last round started; they join the next one.
    spawned: RefCell<Vec<Slot>>,
    /// Every task not yet finished, failed or cancelled.
    live: RefCell<BTreeSet<u64>>,
    /// Cancelled while their round was running; dropped when reached.
    cancelled: RefCell<BTreeSet<u64>>,
    next_id: Cell<u64>,
}

impl Executor {
    pub const fn new() -> Executor {
        Executor {
            tasks: RefCell::new(Vec::new()),
            spawned: RefCell::new(Vec::new()),
            live: RefCell::new(BTreeSet::new()),
            cancelled: RefCell::new(BTreeSet::new()),
            next_id: Cell::new(1),
        }
    }

    pub fn next_id(&self) -> TaskId {
        let id = self.next_id.get();
        self.next_id.set(id + 1);
        TaskId(id)
    }

    pub fn spawn(&self, id: TaskId, future: BoxedTask) {
        self.live.borrow_mut().insert(id.0);
        self.spawned.borrow_mut().push(Slot { id: id.0, future });
    }

    pub fn len(&self) -> usize {
        self.live.borrow().len()
    }

    /// Stops a task; false if it had already ended. A task waiting in a list
    /// is dropped at once; one in the round being polled, when it's reached.
    pub fn cancel(&self, id: TaskId) -> bool {
        if !self.live.borrow_mut().remove(&id.0) {
            return false;
        }
        let waiting = take_where(&self.spawned, id.0).or_else(|| take_where(&self.tasks, id.0));
        match waiting {
            Some(slot) => drop_quietly(slot),
            None => {
                self.cancelled.borrow_mut().insert(id.0);
            }
        }
        true
    }

    /// Polls every task once. Finished tasks report failures through
    /// `on_failure`; panicking tasks through `on_panic`. Both are removed.
    pub fn poll_all(&self, on_failure: impl Fn(&str), on_panic: impl Fn(&(dyn Any + Send))) {
        let spawned = std::mem::take(&mut *self.spawned.borrow_mut());
        let mut round = std::mem::take(&mut *self.tasks.borrow_mut());
        round.extend(spawned);

        let mut context = Context::from_waker(Waker::noop());
        let mut still_pending = Vec::with_capacity(round.len());
        for mut slot in round {
            if self.cancelled.borrow_mut().remove(&slot.id) {
                drop_quietly(slot);
                continue;
            }
            match catch_unwind(AssertUnwindSafe(|| slot.future.as_mut().poll(&mut context))) {
                Ok(Poll::Pending) => {
                    if self.cancelled.borrow_mut().remove(&slot.id) {
                        drop_quietly(slot); // cancelled itself
                    } else {
                        still_pending.push(slot);
                    }
                }
                Ok(Poll::Ready(failure)) => {
                    self.live.borrow_mut().remove(&slot.id);
                    self.cancelled.borrow_mut().remove(&slot.id);
                    drop_quietly(slot);
                    if let Some(failure) = failure {
                        on_failure(&failure);
                    }
                }
                Err(payload) => {
                    self.live.borrow_mut().remove(&slot.id);
                    self.cancelled.borrow_mut().remove(&slot.id);
                    drop_quietly(slot);
                    on_panic(payload.as_ref());
                }
            }
        }
        // Nothing else adds to `tasks` while a round runs.
        self.tasks.borrow_mut().extend(still_pending);
    }

    /// Drops every task (on shutdown).
    pub fn clear(&self) {
        let slots: Vec<_> = self.tasks.borrow_mut().drain(..).chain(self.spawned.borrow_mut().drain(..)).collect();
        self.live.borrow_mut().clear();
        self.cancelled.borrow_mut().clear();
        for slot in slots {
            drop_quietly(slot);
        }
    }
}

fn take_where(list: &RefCell<Vec<Slot>>, id: u64) -> Option<Slot> {
    let mut list = list.borrow_mut();
    let index = list.iter().position(|slot| slot.id == id)?;
    Some(list.remove(index))
}

/// Drops a task's future, whose destructors are plugin code and may panic.
fn drop_quietly(slot: Slot) {
    let _ = catch_unwind(AssertUnwindSafe(move || drop(slot)));
}

impl<P: Plugin> Mq<P> {
    /// Starts an async task, first polled on the next pulse (or this one, if
    /// called from `on_pulse`). See [`Task`] for what a task can do.
    pub fn spawn_task<F, Fut>(&self, start: F) -> TaskId
    where
        F: FnOnce(Task<P>) -> Fut,
        Fut: Future<Output = ()> + 'static,
    {
        self.spawn_boxed(|task| {
            let future = start(task);
            Box::pin(async move {
                future.await;
                None
            })
        })
    }

    /// Like [`Mq::spawn_task`], for a task that returns a `Result`: an error
    /// ends the task and is reported in chat, so `?` works inside it.
    pub fn spawn_fallible_task<F, Fut, E>(&self, start: F) -> TaskId
    where
        F: FnOnce(Task<P>) -> Fut,
        Fut: Future<Output = Result<(), E>> + 'static,
        E: fmt::Display,
    {
        self.spawn_boxed(|task| {
            let future = start(task);
            Box::pin(async move { future.await.err().map(|e| e.to_string()) })
        })
    }

    fn spawn_boxed(&self, start: impl FnOnce(Task<P>) -> BoxedTask) -> TaskId {
        let runtime = self.runtime();
        let id = runtime.executor().next_id();
        let future = start(Task { runtime, id, _not_send: PhantomData });
        runtime.executor().spawn(id, future);
        id
    }

    /// Stops a task. False if it had already finished.
    pub fn cancel_task(&self, id: TaskId) -> bool {
        self.runtime().executor().cancel(id)
    }

    /// Tasks spawned and not yet finished.
    pub fn task_count(&self) -> usize {
        self.runtime().executor().len()
    }
}
