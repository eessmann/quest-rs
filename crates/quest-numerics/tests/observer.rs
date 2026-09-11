use googletest::prelude::*;
use quest_numerics::observer::{
    Clock, NoopObserver, Observer, Stage, TraceObserver, observe_result,
};
use std::{cell::Cell, time::Duration};
struct CounterClock {
    calls: Cell<u64>,
}
impl Clock for CounterClock {
    fn now(&self) -> Duration {
        let n = self.calls.get();
        self.calls.set(n.saturating_add(1));
        Duration::from_nanos(n.saturating_mul(10))
    }
}
#[gtest]
fn compile_time_noop_observer_never_reads_the_clock() {
    let clock = CounterClock {
        calls: Cell::new(0),
    };
    let mut observer = NoopObserver;
    let result: std::result::Result<usize, ()> =
        observe_result(&mut observer, &clock, Stage::Execution, || Ok(7));
    expect_that!(result, ok(eq(7)));
    expect_that!(clock.calls.get(), eq(0));
    expect_false!(NoopObserver::ENABLED);
}
#[gtest]
fn owned_trace_records_success_errors_and_early_drop() {
    let clock = CounterClock {
        calls: Cell::new(0),
    };
    let mut trace = TraceObserver::new(8);
    let _: std::result::Result<(), ()> =
        observe_result(&mut trace, &clock, Stage::Synthesis, || Ok(()));
    let _: std::result::Result<(), ()> =
        observe_result(&mut trace, &clock, Stage::Certification, || Err(()));
    {
        let _span = quest_numerics::observer::Span::new(&mut trace, &clock, Stage::Offline);
    }
    expect_that!(trace.events().len(), eq(3));
    expect_that!(
        trace
            .events()
            .first()
            .map(quest_numerics::observer::Event::elapsed),
        some(eq(Duration::from_nanos(10)))
    );
    expect_that!(trace.dropped_events(), eq(0));
}
#[gtest]
fn trace_outcomes_and_exhaustion_are_explicit() {
    use quest_numerics::observer::{Outcome, Span};
    let clock = CounterClock {
        calls: Cell::new(0),
    };
    let mut trace = TraceObserver::new(3);
    Span::new(&mut trace, &clock, Stage::Approximation).finish();
    Span::new(&mut trace, &clock, Stage::Certification).fail();
    {
        let _span = Span::new(&mut trace, &clock, Stage::Offline);
    }
    Span::new(&mut trace, &clock, Stage::Execution).finish();
    expect_that!(
        trace
            .events()
            .iter()
            .map(quest_numerics::observer::Event::outcome)
            .collect::<Vec<_>>(),
        elements_are![
            eq(&Outcome::Completed),
            eq(&Outcome::Failed),
            eq(&Outcome::Aborted)
        ]
    );
    expect_that!(trace.dropped_events(), eq(1));
    trace.clear();
    expect_true!(trace.events().is_empty());
    expect_that!(trace.dropped_events(), eq(0));
}
#[gtest]
fn backward_caller_clock_is_marked_instead_of_underflowing() {
    struct BackwardClock(Cell<bool>);
    impl Clock for BackwardClock {
        fn now(&self) -> Duration {
            if self.0.replace(true) {
                Duration::ZERO
            } else {
                Duration::from_secs(1)
            }
        }
    }
    let clock = BackwardClock(Cell::new(false));
    let mut trace = TraceObserver::new(1);
    quest_numerics::observer::Span::new(&mut trace, &clock, Stage::Preparation).finish();
    expect_true!(
        trace
            .events()
            .first()
            .is_some_and(quest_numerics::observer::Event::clock_regressed)
    );
    expect_that!(
        trace
            .events()
            .first()
            .map(quest_numerics::observer::Event::elapsed),
        some(eq(Duration::ZERO))
    );
}
#[gtest]
fn nested_scopes_include_the_complete_operation_and_its_retry_loop() -> Result<()> {
    use quest_numerics::observer::Span;
    let clock = CounterClock {
        calls: Cell::new(0),
    };
    let mut trace = TraceObserver::new(8);
    let mut total = Span::new(&mut trace, &clock, Stage::EndToEnd);
    observe_result(
        total.observer_mut(),
        &clock,
        Stage::Certification,
        || -> std::result::Result<(), ()> {
            for _ in 0..3 {
                clock.now();
            }
            Ok(())
        },
    )
    .map_err(|()| std::io::Error::other("unexpected fixture error"))?;
    total.finish();
    expect_that!(
        trace
            .events()
            .first()
            .map(quest_numerics::observer::Event::elapsed),
        some(eq(Duration::from_nanos(40)))
    );
    expect_that!(
        trace
            .events()
            .last()
            .map(quest_numerics::observer::Event::elapsed),
        some(eq(Duration::from_nanos(60)))
    );
    Ok(())
}
#[cfg(feature = "trace-json")]
#[gtest]
fn chrome_json_contains_typed_stage_outcomes_and_trace_loss() -> Result<()> {
    let clock = CounterClock {
        calls: Cell::new(0),
    };
    let mut trace = TraceObserver::new(1);
    quest_numerics::observer::Span::new(&mut trace, &clock, Stage::Lowering).fail();
    quest_numerics::observer::Span::new(&mut trace, &clock, Stage::Postselection).finish();
    let value: serde_json::Value = serde_json::from_str(&trace.to_chrome_json()?)?;
    let event = value
        .get("traceEvents")
        .and_then(serde_json::Value::as_array)
        .and_then(|a| a.first())
        .ok_or_else(|| std::io::Error::other("missing event"))?;
    expect_that!(
        event.get("ph").and_then(serde_json::Value::as_str),
        some(eq("X"))
    );
    expect_that!(
        event.get("name").and_then(serde_json::Value::as_str),
        some(eq("lowering"))
    );
    expect_that!(
        event
            .get("args")
            .and_then(|a| a.get("outcome"))
            .and_then(serde_json::Value::as_str),
        some(eq("failed"))
    );
    expect_that!(
        value
            .get("otherData")
            .and_then(|a| a.get("dropped_events"))
            .and_then(serde_json::Value::as_u64),
        some(eq(1))
    );
    Ok(())
}
