#[cfg(not(quest_target_policy))]
compile_error!("required target compiler policy missing");
#[cfg(quest_host_policy)]
compile_error!("host compiler policy leaked into target build");

fn main() {
	let half = std::hint::black_box(f64::MIN_POSITIVE) * std::hint::black_box(0.5);
	assert!(half > 0.0, "target flushes subnormals");
	println!("separate host and target compiler policies executed");
}
