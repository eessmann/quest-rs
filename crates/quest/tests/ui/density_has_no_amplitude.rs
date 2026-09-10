use quest::{Environment, QubitCount};
fn main() {
    let environment = Environment::builder().build().unwrap();
    let register = environment.density_matrix(QubitCount::new(1).unwrap()).unwrap();
    register.amplitude(0).unwrap();
}
