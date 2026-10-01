#[path = "support/function_measurement.rs"]
mod measurement;
#[allow(
    clippy::arithmetic_side_effects,
    reason = "These operators construct expression nodes; backend execution is fallible."
)]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    measurement::run("dynamic", || {
        quest_polynomial::function!(|x| (x.clone() * x + 1.0).ln())
    })
}
