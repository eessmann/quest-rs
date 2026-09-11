use quest::{Environment, circuit};
fn main() {
    let environment = Environment::builder().build().unwrap();
    let prepared = environment.prepare_structured(circuit! { qubit q; reset q; }.unwrap()).unwrap();
    drop(environment);
    drop(prepared);
}
