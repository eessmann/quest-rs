use quest_compile::{circuit, OracleFragment};
fn forbidden(fragment: OracleFragment) {
    let _ = circuit! {
        oracle block[1] = ${fragment};
        qubit q;
        inv @ block q;
    };
}
fn main() {}
