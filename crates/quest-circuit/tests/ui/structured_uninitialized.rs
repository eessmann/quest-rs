use quest_circuit::circuit;
fn main() {
    let _ = circuit! { input bool choose; int value; if (choose) { value=1; } int result=value; };
}
