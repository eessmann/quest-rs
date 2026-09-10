use quest_circuit::legacy_circuit as circuit;
fn main() { let _ = circuit! { qubit q; rx(1e999) q; }; }
