//! Historical seven-row initial weak-generator entry; matrix and schema unchanged.
#[path = "support/configuration_weak_protocol.rs"]
mod protocol;
fn main() -> Result<(), Box<dyn std::error::Error>> {
	protocol::main_entry(protocol::Protocol::SevenRows)
}
