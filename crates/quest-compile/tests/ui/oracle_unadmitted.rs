use quest_compile::{BoundRegion, OracleFragment};
fn forbidden(program: BoundRegion) {
    let _ = OracleFragment::builder(program).build();
}
fn main() {}
