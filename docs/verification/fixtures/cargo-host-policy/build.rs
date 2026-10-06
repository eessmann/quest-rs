#[cfg(not(quest_host_policy))]
compile_error!("required host compiler policy missing");
#[cfg(quest_target_policy)]
compile_error!("target compiler policy leaked into host build");

fn main() {
	let half = std::hint::black_box(f64::MIN_POSITIVE) * std::hint::black_box(0.5);
	assert!(half > 0.0, "host flushes subnormals");
	println!("cargo:rerun-if-changed=build.rs");
}
