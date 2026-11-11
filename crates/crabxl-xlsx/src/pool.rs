//! One pool of worker threads for the whole process.
//!
//! Threads start only when a task arrives and every started thread is busy,
//! up to the CPUs this process may use, minus the thread that calls CrabXL.
//! Set `CRABXL_MAX_THREADS` or call [`set_max_threads`] to use fewer.

use std::{
    collections::VecDeque,
    num::NonZero,
    panic::{AssertUnwindSafe, catch_unwind},
    sync::{
        Condvar, Mutex, OnceLock,
        atomic::{AtomicUsize, Ordering},
    },
    thread,
};

type Task = Box<dyn FnOnce() + Send>;

#[derive(Default)]
struct Pool {
    state: Mutex<State>,
    wake: Condvar,
}
#[derive(Default)]
struct State {
    tasks: VecDeque<Task>,
    threads: usize,
    idle: usize,
}

static POOL: OnceLock<Pool> = OnceLock::new();
/// The configured thread limit; 0 means not set.
static MAX_THREADS: AtomicUsize = AtomicUsize::new(0);

/// Use at most `threads` threads at once, counting the calling thread.
/// 1 reads and writes on the calling thread only. 0 restores the default:
/// `CRABXL_MAX_THREADS`, or else the CPUs this process may use.
pub fn set_max_threads(threads: usize) {
    MAX_THREADS.store(threads, Ordering::Relaxed);
}

/// The most threads CrabXL uses at once, counting the calling thread.
pub fn max_threads() -> usize {
    match MAX_THREADS.load(Ordering::Relaxed) {
        0 => default_threads(),
        threads => threads,
    }
}

fn default_threads() -> usize {
    static DEFAULT: OnceLock<usize> = OnceLock::new();
    *DEFAULT.get_or_init(|| {
        std::env::var("CRABXL_MAX_THREADS")
            .ok()
            .and_then(|value| value.trim().parse::<usize>().ok())
            .filter(|threads| *threads > 0)
            .unwrap_or_else(allowed_cpus)
    })
}

/// CPUs this process may run on. On Linux the standard library already
/// honors affinity and container limits; on Windows it counts every CPU, so
/// the process affinity mask is read instead.
fn allowed_cpus() -> usize {
    let cpus = thread::available_parallelism().map_or(1, NonZero::get);
    #[cfg(windows)]
    if let Some(cores) = core_affinity::get_core_ids().filter(|cores| !cores.is_empty()) {
        return cores.len().min(cpus);
    }
    cpus
}

/// Worker threads available for parallel work; 0 means work on one thread.
pub(crate) fn workers() -> usize {
    max_threads().saturating_sub(1)
}

/// Run `task` on a worker thread.
pub(crate) fn spawn(task: impl FnOnce() + Send + 'static) {
    let pool = POOL.get_or_init(Pool::default);
    let Ok(mut state) = pool.state.lock() else {
        return;
    };
    state.tasks.push_back(Box::new(task));
    if state.idle == 0 && state.threads < workers().max(1) {
        let started = thread::Builder::new()
            .name("crabxl-worker".into())
            .spawn(|| run(pool));
        if started.is_ok() {
            state.threads += 1;
        }
    }
    drop(state);
    pool.wake.notify_one();
}

fn run(pool: &'static Pool) {
    loop {
        let task = {
            let Ok(mut state) = pool.state.lock() else {
                return;
            };
            loop {
                if let Some(task) = state.tasks.pop_front() {
                    break task;
                }
                state.idle += 1;
                state = match pool.wake.wait(state) {
                    Ok(state) => state,
                    Err(_) => return,
                };
                state.idle -= 1;
            }
        };
        // A panicking task drops its result sender; its owner sees that.
        let _ = catch_unwind(AssertUnwindSafe(task));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc::channel;

    #[test]
    fn tasks_run_and_a_panic_does_not_stop_the_pool() {
        let (done, results) = channel();
        let sender = done.clone();
        spawn(move || {
            let _keep = sender;
            panic!("task failed");
        });
        for value in 0..20 {
            let done = done.clone();
            spawn(move || {
                let _ = done.send(value * 2);
            });
        }
        drop(done);
        let mut values: Vec<_> = results.iter().collect();
        values.sort_unstable();
        assert_eq!(values, (0..20).map(|value| value * 2).collect::<Vec<_>>());
        assert!(allowed_cpus() >= 1);
    }
}
