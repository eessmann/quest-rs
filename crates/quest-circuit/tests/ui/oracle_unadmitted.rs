use quest_circuit::{BoundProgram, OracleFragment};
fn forbidden(program: BoundProgram) {
    let _ = OracleFragment::builder(program).build();
}
fn main() {}
