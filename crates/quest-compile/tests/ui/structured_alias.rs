use quest_compile::circuit;
fn main() {
    let _ = circuit! { def change(mutable array[int,2] first, readonly array[int,2] second) { first[0]=second[0]; }
        array[int,2] data={0,1}; change(data,data); };
}
