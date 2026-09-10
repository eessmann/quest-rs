use quest::{Environment, legacy_circuit};
fn main() {
    let environment = Environment::builder().build().unwrap();
    let prepared = environment.prepare(legacy_circuit! { qubit q; h q; }.unwrap()).unwrap();
    drop(environment);
    drop(prepared);
}
