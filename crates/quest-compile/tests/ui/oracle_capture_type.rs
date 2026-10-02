use quest_compile::circuit;
fn main() {
    let _ = circuit! {
        oracle block[1] = ${0.5};
        qubit q;
        block q;
    };
}
