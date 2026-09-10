use quest_circuit::circuit;
fn main() {
    let _ = circuit! { qubit q; rx(${true}) q; };
}
