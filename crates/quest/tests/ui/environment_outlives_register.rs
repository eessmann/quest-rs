use quest::{Environment, QubitCount};
fn main() {
    let environment = Environment::builder().build().unwrap();
    let mut register = environment.state_vector(QubitCount::new(1).unwrap()).unwrap();
    drop(environment);
    register.h(0).unwrap();
}
