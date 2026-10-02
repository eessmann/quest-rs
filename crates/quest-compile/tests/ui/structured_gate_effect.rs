use quest_compile::circuit;
fn main() {
    let _ = circuit! { gate bad q { reset q; } qubit q; bad q; };
}
