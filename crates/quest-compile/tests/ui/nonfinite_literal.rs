use quest_compile::circuit;
fn main() { let _ = circuit! { qubit q; rx(1e999) q; }; }
