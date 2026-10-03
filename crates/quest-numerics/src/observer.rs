//! Caller-owned, synchronous stage observation without global hooks.
//!
//! Wrap the entire operation to include its retries. In particular, observing a
//! certification builder's `certify()` call includes every precision attempt.
//! Durations describe the caller's clock; the provided monotonic clock measures
//! elapsed wall time on the calling CPU, not GPU completion or process CPU usage.
//!
//! ```
//! use quest_numerics::observer::{MonotonicClock, Stage, TraceObserver, observe_result};
//! let clock = MonotonicClock::new();
//! let mut trace = TraceObserver::new(16);
//! let value: Result<usize, ()> = observe_result(&mut trace, &clock,
//!     Stage::Construction, || Ok(7));
//! assert_eq!(value, Ok(7));
//! assert_eq!(trace.events().len(), 1);
//! ```
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum Stage {
	Approximation,
	Synthesis,
	Certification,
	Offline,
	Construction,
	Lowering,
	Preparation,
	Execution,
	Postselection,
	/// Application dispatch after runtime and worker-pool admission.
	ApplicationDispatch,
	EndToEnd,
}
impl Stage {
	#[must_use]
	pub const fn name(self) -> &'static str {
		match self {
			Self::Approximation => "approximation",
			Self::Synthesis => "synthesis",
			Self::Certification => "certification",
			Self::Offline => "offline",
			Self::Construction => "construction",
			Self::Lowering => "lowering",
			Self::Preparation => "preparation",
			Self::Execution => "execution",
			Self::Postselection => "postselection",
			Self::ApplicationDispatch => "application_dispatch",
			Self::EndToEnd => "end_to_end",
		}
	}
}
/// Caller-selected clock, with a common epoch for spans written into one trace.
/// Implementations should be monotonic. Regressions are explicitly marked in events.
pub trait Clock {
	fn now(&self) -> Duration;
}
#[derive(Debug, Clone)]
pub struct MonotonicClock {
	origin: Instant,
}
impl Default for MonotonicClock {
	fn default() -> Self {
		Self::new()
	}
}
impl MonotonicClock {
	#[must_use]
	pub fn new() -> Self {
		Self {
			origin: Instant::now(),
		}
	}
}
impl Clock for MonotonicClock {
	fn now(&self) -> Duration {
		self.origin.elapsed()
	}
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
	Completed,
	Failed,
	Aborted,
}
impl Outcome {
	#[must_use]
	pub const fn name(self) -> &'static str {
		match self {
			Self::Completed => "completed",
			Self::Failed => "failed",
			Self::Aborted => "aborted",
		}
	}
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Event {
	stage: Stage,
	start: Duration,
	elapsed: Duration,
	outcome: Outcome,
	clock_regressed: bool,
}
impl Event {
	#[must_use]
	pub const fn stage(&self) -> Stage {
		self.stage
	}
	#[must_use]
	pub const fn start(&self) -> Duration {
		self.start
	}
	#[must_use]
	pub const fn elapsed(&self) -> Duration {
		self.elapsed
	}
	#[must_use]
	pub const fn outcome(&self) -> Outcome {
		self.outcome
	}
	#[must_use]
	pub const fn clock_regressed(&self) -> bool {
		self.clock_regressed
	}
}
/// Receives complete owned events synchronously. Callbacks must not panic in Drop.
pub trait Observer {
	/// A compile-time false disables clock reads and event construction entirely.
	const ENABLED: bool = true;
	fn record(&mut self, event: Event);
}
#[derive(Debug, Clone, Copy, Default)]
pub struct NoopObserver;
impl Observer for NoopObserver {
	const ENABLED: bool = false;
	fn record(&mut self, _event: Event) {}
}
/// An unfinished span records Aborted on early return or unwinding.
/// `finish` and `fail` consume the span, recording exactly one event.
#[must_use = "hold the span through the measured operation"]
pub struct Span<'a, O: Observer, C: Clock> {
	observer: &'a mut O,
	clock: &'a C,
	stage: Stage,
	start: Option<Duration>,
}
impl<'a, O: Observer, C: Clock> Span<'a, O, C> {
	pub fn new(observer: &'a mut O, clock: &'a C, stage: Stage) -> Self {
		Self {
			observer,
			clock,
			stage,
			start: if O::ENABLED { Some(clock.now()) } else { None },
		}
	}
	/// Reborrow the observer for nested spans on the same synchronous trace.
	pub const fn observer_mut(&mut self) -> &mut O {
		self.observer
	}
	pub fn finish(mut self) {
		self.close(Outcome::Completed);
	}
	pub fn fail(mut self) {
		self.close(Outcome::Failed);
	}
	fn close(&mut self, outcome: Outcome) {
		if let Some(start) = self.start.take() {
			let elapsed = self.clock.now().checked_sub(start);
			self.observer.record(Event {
				stage: self.stage,
				start,
				elapsed: elapsed.unwrap_or(Duration::ZERO),
				outcome,
				clock_regressed: elapsed.is_none(),
			});
		}
	}
}
impl<O: Observer, C: Clock> Drop for Span<'_, O, C> {
	fn drop(&mut self) {
		self.close(Outcome::Aborted);
	}
}
/// Measures an entire operation, including its internal numerical retries.
/// # Errors
/// Returns the operation's original error without wrapping or suppressing it.
pub fn observe_result<O: Observer, C: Clock, T, E>(
	observer: &mut O,
	clock: &C,
	stage: Stage,
	operation: impl FnOnce() -> Result<T, E>,
) -> Result<T, E> {
	let span = Span::new(observer, clock, stage);
	let result = operation();
	if result.is_ok() {
		span.finish();
	} else {
		span.fail();
	}
	result
}
/// Bounded in-memory events. Exhaustion is reported as dropped events, not hidden.
#[derive(Debug)]
pub struct TraceObserver {
	events: Vec<Event>,
	maximum: usize,
	dropped: usize,
}
impl TraceObserver {
	#[must_use]
	pub const fn new(maximum_events: usize) -> Self {
		Self {
			events: Vec::new(),
			maximum: maximum_events,
			dropped: 0,
		}
	}
	#[must_use]
	pub fn events(&self) -> &[Event] {
		&self.events
	}
	#[must_use]
	pub const fn dropped_events(&self) -> usize {
		self.dropped
	}
	pub fn clear(&mut self) {
		self.events.clear();
		self.dropped = 0;
	}
	/// Chrome trace-event JSON accepted by Perfetto. Complete events use integer
	/// microseconds; submicrosecond timing remains available in `events()`.
	/// The exporter is cold tooling behind `trace-json` and reports dropped events.
	/// # Errors
	/// Propagates JSON serialization errors.
	#[cfg(feature = "trace-json")]
	pub fn to_chrome_json(&self) -> Result<String, serde_json::Error> {
		let events: Vec<_> = self
			.events
			.iter()
			.map(|event| {
				serde_json::json!({
					"name":event.stage.name(), "cat":"quest.stage", "ph":"X",
					"ts":event.start.as_micros(), "dur":event.elapsed.as_micros(),
					"pid":0, "tid":0,
					"args": { "outcome":event.outcome.name(), "clock_regressed":event.clock_regressed }
				})
			})
			.collect();
		serde_json::to_string(&serde_json::json!({
			"traceEvents":events, "displayTimeUnit":"ms",
			"otherData": { "dropped_events":self.dropped }
		}))
	}
}
impl Observer for TraceObserver {
	fn record(&mut self, event: Event) {
		if self.events.len() >= self.maximum || self.events.try_reserve(1).is_err() {
			self.dropped = self.dropped.saturating_add(1);
		} else {
			self.events.push(event);
		}
	}
}
