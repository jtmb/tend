//! One sleeping, demand-driven collector. Every subscriber has a single latest
//! slot, so slow views never build an unbounded sample queue.
use crate::{
    observations::Observation,
    task_stats::{self, Snapshot},
    workstation::{self, Sample},
};
use std::{
    collections::BTreeMap,
    sync::{Arc, Condvar, Mutex, Weak},
    thread::JoinHandle,
    time::{Duration, Instant},
};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum Detail {
    Basic,
    Full,
}
pub struct Capture {
    pub sequence: u64,
    pub observation: Observation,
    pub full: Option<Arc<Snapshot>>,
}
pub type Update = Result<Arc<Capture>, String>;
pub trait Provider: Send + 'static {
    fn collect(
        &mut self,
        detail: Detail,
        previous: Option<&Snapshot>,
    ) -> Result<(Sample, Option<Snapshot>), String>;
}
pub struct LiveProvider;
impl Provider for LiveProvider {
    fn collect(
        &mut self,
        detail: Detail,
        previous: Option<&Snapshot>,
    ) -> Result<(Sample, Option<Snapshot>), String> {
        if detail == Detail::Full {
            let snapshot = task_stats::collect(previous);
            Ok((snapshot.system.clone(), Some(snapshot)))
        } else {
            Ok((workstation::sample(), None))
        }
    }
}
#[derive(Default)]
struct Slot {
    value: Mutex<Option<Update>>,
    ready: Condvar,
}
impl Slot {
    fn put(&self, value: Update) {
        *self.value.lock().unwrap() = Some(value);
        self.ready.notify_all();
    }
}
struct Request {
    detail: Detail,
    period: Duration,
    next: Instant,
    once: bool,
    slot: Arc<Slot>,
}
#[derive(Default)]
struct State {
    next_id: u64,
    requests: BTreeMap<u64, Request>,
    stop: bool,
    collections: u64,
}
struct Inner {
    state: Mutex<State>,
    changed: Condvar,
}
struct Owner {
    inner: Arc<Inner>,
    thread: Mutex<Option<JoinHandle<()>>>,
}
impl Drop for Owner {
    fn drop(&mut self) {
        self.inner.state.lock().unwrap().stop = true;
        self.inner.changed.notify_all();
        if let Some(thread) = self.thread.lock().unwrap().take() {
            let _ = thread.join();
        }
    }
}
#[derive(Clone)]
pub struct Hub {
    owner: Arc<Owner>,
}
pub struct Subscription {
    id: u64,
    inner: Weak<Inner>,
    slot: Arc<Slot>,
}
impl Drop for Subscription {
    fn drop(&mut self) {
        if let Some(inner) = self.inner.upgrade() {
            inner.state.lock().unwrap().requests.remove(&self.id);
            inner.changed.notify_all();
        }
    }
}
impl Subscription {
    pub fn take_latest(&self) -> Option<Update> {
        self.slot.value.lock().unwrap().take()
    }
    pub fn wait(&self, timeout: Duration) -> Option<Update> {
        let value = self.slot.value.lock().unwrap();
        let (mut value, _) = self
            .slot
            .ready
            .wait_timeout_while(value, timeout, |v| v.is_none())
            .unwrap();
        value.take()
    }
}
impl Hub {
    pub fn new() -> Self {
        Self::with_provider(LiveProvider, || {})
    }
    pub fn with_notifier(notify: impl Fn() + Send + Sync + 'static) -> Self {
        Self::with_provider(LiveProvider, notify)
    }
    pub fn with_provider(
        mut provider: impl Provider,
        notify: impl Fn() + Send + Sync + 'static,
    ) -> Self {
        let inner = Arc::new(Inner {
            state: Mutex::new(State::default()),
            changed: Condvar::new(),
        });
        let worker = inner.clone();
        let thread = std::thread::Builder::new()
            .name("tend-sampler".into())
            .spawn(move || {
                // Linux niceness is per thread. Yield background collection
                // under contention without lowering the GTK/input thread.
                // This only lowers priority and needs no elevated privilege;
                // unsupported/restricted environments keep their current value.
                #[cfg(target_os = "linux")]
                unsafe {
                    libc::nice(5);
                }
                let mut previous_full: Option<Arc<Snapshot>> = None;
                let mut previous_basic: Option<(Sample, Instant, Option<u64>)> = None;
                loop {
                    let mut state = worker.state.lock().unwrap();
                    while !state.stop && state.requests.is_empty() {
                        previous_full = None;
                        previous_basic = None;
                        state = worker.changed.wait(state).unwrap();
                    }
                    if state.stop {
                        break;
                    }
                    let now = Instant::now();
                    let next = state.requests.values().map(|r| r.next).min().unwrap();
                    if next > now {
                        let _ = worker.changed.wait_timeout(state, next - now).unwrap();
                        continue;
                    }
                    let detail = state
                        .requests
                        .values()
                        .filter(|r| r.next <= now)
                        .map(|r| r.detail)
                        .max()
                        .unwrap();
                    let previous = previous_full
                        .as_deref()
                        .filter(|p| now.duration_since(p.at) <= Duration::from_secs(60));
                    drop(state);
                    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                        provider.collect(detail, previous)
                    }))
                    .unwrap_or_else(|_| {
                        Err("Resource collection stopped unexpectedly; refresh to retry.".into())
                    });
                    let finished = Instant::now();
                    let boot_finished = crate::observations::boot_millis();
                    let mut state = worker.state.lock().unwrap();
                    if state.stop {
                        break;
                    }
                    state.collections += 1;
                    let capture = result.map(|(basic, full)| {
                        let (interval, gap) = previous_basic
                            .as_ref()
                            .map(|(_, at, boot)| {
                                crate::observations::sample_interval(
                                    *boot,
                                    boot_finished,
                                    finished.duration_since(*at),
                                )
                            })
                            .unwrap_or((Duration::ZERO, true));
                        let mut observation = Observation::from_samples(
                            &basic,
                            previous_basic.as_ref().filter(|_| !gap).map(|(s, _, _)| s),
                            interval,
                        );
                        observation.gap = gap;
                        previous_basic = Some((basic, finished, boot_finished));
                        let full = full.map(Arc::new);
                        if let Some(full) = &full {
                            previous_full = Some(full.clone());
                        }
                        Arc::new(Capture {
                            sequence: state.collections,
                            observation,
                            full,
                        })
                    });
                    let mut once = Vec::new();
                    let mut delivered = false;
                    for (id, request) in &mut state.requests {
                        if request.next <= finished && request.detail <= detail {
                            request.slot.put(capture.clone());
                            request.next = finished + request.period;
                            delivered = true;
                            if request.once {
                                once.push(*id);
                            }
                        }
                    }
                    for id in once {
                        state.requests.remove(&id);
                    }
                    drop(state);
                    if delivered {
                        notify();
                    }
                }
            })
            .expect("Could not create the resource collector");
        Self {
            owner: Arc::new(Owner {
                inner,
                thread: Mutex::new(Some(thread)),
            }),
        }
    }
    pub fn subscribe(&self, detail: Detail, period: Duration) -> Subscription {
        self.add(detail, period, false)
    }
    pub fn request(&self, detail: Detail) -> Subscription {
        self.add(detail, Duration::ZERO, true)
    }
    fn add(&self, detail: Detail, period: Duration, once: bool) -> Subscription {
        let inner = &self.owner.inner;
        let mut state = inner.state.lock().unwrap();
        state.next_id += 1;
        let id = state.next_id;
        let slot = Arc::new(Slot::default());
        state.requests.insert(
            id,
            Request {
                detail,
                period: period.max(Duration::from_millis(10)),
                next: Instant::now(),
                once,
                slot: slot.clone(),
            },
        );
        inner.changed.notify_all();
        Subscription {
            id,
            inner: Arc::downgrade(inner),
            slot,
        }
    }
    pub fn shutdown(&self) {
        let inner = &self.owner.inner;
        let mut state = inner.state.lock().unwrap();
        state.stop = true;
        for request in state.requests.values() {
            request.slot.put(Err("Monitoring stopped.".into()));
        }
        state.requests.clear();
        inner.changed.notify_all();
    }
    pub fn collection_count(&self) -> u64 {
        self.owner.inner.state.lock().unwrap().collections
    }
    pub fn subscriber_count(&self) -> usize {
        self.owner.inner.state.lock().unwrap().requests.len()
    }
}
impl Default for Hub {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    struct Fake(Arc<AtomicUsize>);
    impl Provider for Fake {
        fn collect(
            &mut self,
            _: Detail,
            _: Option<&Snapshot>,
        ) -> Result<(Sample, Option<Snapshot>), String> {
            let n = self.0.fetch_add(1, Ordering::SeqCst) as u64;
            Ok((
                Sample {
                    total: 100 + n * 100,
                    idle: 50 + n * 50,
                    ..Default::default()
                },
                None,
            ))
        }
    }
    #[cfg(target_os = "linux")]
    #[test]
    fn collector_yields_without_changing_the_requesting_threads_priority() {
        use std::sync::atomic::AtomicI32;
        struct PriorityProbe(Arc<AtomicI32>);
        impl Provider for PriorityProbe {
            fn collect(
                &mut self,
                _: Detail,
                _: Option<&Snapshot>,
            ) -> Result<(Sample, Option<Snapshot>), String> {
                self.0.store(
                    unsafe { libc::getpriority(libc::PRIO_PROCESS, 0) },
                    Ordering::SeqCst,
                );
                Ok((Sample::default(), None))
            }
        }
        let caller = unsafe { libc::getpriority(libc::PRIO_PROCESS, 0) };
        let observed = Arc::new(AtomicI32::new(i32::MIN));
        let hub = Hub::with_provider(PriorityProbe(observed.clone()), || {});
        assert!(hub
            .request(Detail::Basic)
            .wait(Duration::from_secs(2))
            .unwrap()
            .is_ok());
        assert_eq!(observed.load(Ordering::SeqCst), (caller + 5).min(19));
        assert_eq!(unsafe { libc::getpriority(libc::PRIO_PROCESS, 0) }, caller);
        hub.shutdown();
    }
    #[test]
    fn sleeps_with_no_demand_and_stops_after_last_subscription() {
        let calls = Arc::new(AtomicUsize::new(0));
        let hub = Hub::with_provider(Fake(calls.clone()), || {});
        std::thread::sleep(Duration::from_millis(25));
        assert_eq!(calls.load(Ordering::SeqCst), 0);
        let sub = hub.subscribe(Detail::Basic, Duration::from_secs(30));
        assert!(sub.wait(Duration::from_secs(2)).unwrap().is_ok());
        drop(sub);
        let before = calls.load(Ordering::SeqCst);
        std::thread::sleep(Duration::from_millis(25));
        assert_eq!(calls.load(Ordering::SeqCst), before);
        assert_eq!(hub.subscriber_count(), 0);
    }
    #[test]
    fn queue_is_one_slot_and_one_shot_demand_is_removed() {
        let hub = Hub::with_provider(Fake(Arc::new(AtomicUsize::new(0))), || {});
        let sub = hub.request(Detail::Full);
        let capture = sub.wait(Duration::from_secs(2)).unwrap().unwrap();
        assert_eq!(capture.sequence, 1);
        assert_eq!(hub.subscriber_count(), 0);
        assert!(sub.take_latest().is_none());
        let sub = hub.subscribe(Detail::Basic, Duration::from_millis(10));
        std::thread::sleep(Duration::from_millis(80));
        assert!(sub.take_latest().unwrap().unwrap().sequence > 2);
        assert!(sub.take_latest().is_none());
        hub.shutdown();
    }
    #[test]
    fn requests_arriving_during_collection_share_the_in_flight_sample() {
        use std::sync::mpsc;
        struct Gate {
            entered: mpsc::SyncSender<()>,
            release: mpsc::Receiver<()>,
        }
        impl Provider for Gate {
            fn collect(
                &mut self,
                _: Detail,
                _: Option<&Snapshot>,
            ) -> Result<(Sample, Option<Snapshot>), String> {
                self.entered.send(()).unwrap();
                self.release.recv().unwrap();
                Ok((Sample::default(), None))
            }
        }
        let (entered_tx, entered_rx) = mpsc::sync_channel(1);
        let (release_tx, release_rx) = mpsc::channel();
        let hub = Hub::with_provider(
            Gate {
                entered: entered_tx,
                release: release_rx,
            },
            || {},
        );
        let a = hub.request(Detail::Full);
        entered_rx.recv_timeout(Duration::from_secs(2)).unwrap();
        let b = hub.request(Detail::Basic);
        release_tx.send(()).unwrap();
        assert_eq!(
            a.wait(Duration::from_secs(2)).unwrap().unwrap().sequence,
            b.wait(Duration::from_secs(2)).unwrap().unwrap().sequence
        );
        assert_eq!(hub.collection_count(), 1);
        assert_eq!(hub.subscriber_count(), 0);
    }
}
