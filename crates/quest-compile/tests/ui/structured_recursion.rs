use quest_compile::circuit;
fn main() {
    let _ = circuit! { def recurse(int value) -> int { return recurse(value); } int result=recurse(1); };
}
