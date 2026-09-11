use quest::{Environment, QubitCount};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let snapshot = {
        let environment = Environment::builder().build()?;
        let mut register = environment.state_vector(QubitCount::new(2)?)?;
        register.h(0)?;
        register.cx(0, 1)?;
        register.snapshot()?
    }; // register first, then automatic environment finalization
    println!("Bell state: {snapshot:?}");
    Ok(())
}
