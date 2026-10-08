//! A small complete cover must not allocate the configured worst-case queue.
use quest_numerics::Interval;
use quest_numerics::arithmetic::{First, Interval64Backend};
use quest_numerics::roots::{CoverLimits, Premise, cover};
#[test]
fn continuum_cover_allocates_only_the_used_queue() {
	let input = Interval::new(-1.0, 1.0).unwrap();
	let mut report = None;
	let stats = allocation_counter::measure(|| {
		report = Some(cover(
			&mut Interval64Backend,
			input,
			|_, _| {
				Ok(First {
					value: Interval::point(0.0)?,
					first: Interval::point(0.0)?,
				})
			},
			CoverLimits::default(),
			&1e-10,
			Premise::EnclosesContinuouslyDifferentiableFunction,
		));
	});
	let report = report.unwrap().unwrap();
	assert!(report.complete());
	assert!(report.covered[0].continuum);
	assert!(
		stats.bytes_max <= 1024,
		"a one-box cover must not reserve its 10000-box limit"
	);
}
