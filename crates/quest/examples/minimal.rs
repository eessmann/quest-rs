use quest::{Environment, QubitCount};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let environment = Environment::builder().build()?;
    let mut register = environment.state_vector(QubitCount::new(2)?)?;
    register.h(0)?;
    register.cx(0, 1)?;
    println!("Bell state: {:?}", register.snapshot()?);
    drop(register);
    environment.close()?;
    Ok(())
}
