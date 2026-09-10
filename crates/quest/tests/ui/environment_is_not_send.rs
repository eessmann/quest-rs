use quest::Environment;
fn main() {
    let environment = Environment::builder().build().unwrap();
    std::thread::spawn(move || drop(environment));
}
