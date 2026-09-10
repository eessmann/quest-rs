use quest::{Environment, circuit};
fn main() {
    let environment = Environment::builder().build().unwrap();
    let prepared = environment.prepare(circuit! { qubit q; h q; }.unwrap()).unwrap();
    drop(environment);
    drop(prepared);
}
