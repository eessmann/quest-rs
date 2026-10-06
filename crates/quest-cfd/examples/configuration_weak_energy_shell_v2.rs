//! Additive fixed DG2/two-cell initial energy-shell diagnostic; no history solve.
#[path = "support/configuration_weak_protocol.rs"]
mod protocol;
fn main() -> Result<(), Box<dyn std::error::Error>> {
	protocol::main_entry(protocol::Protocol::EnergyShellV2)
}
