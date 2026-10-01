#[path = "support/function_measurement.rs"]
mod measurement;
#[allow(
    clippy::arithmetic_side_effects,
    reason = "These operators construct expression nodes; backend execution is fallible."
)]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    measurement::run("typed", || {
        quest_polynomial::typed_function!(|x| (x * x + 1.0).ln())
    })
}
