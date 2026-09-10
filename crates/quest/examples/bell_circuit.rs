use quest::{Environment, QubitCount, RunInputs, circuit};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let bell = circuit! {
        qubit[2] q;
        output bit[2] c;
        h q[0];
        cx q[0], q[1];
        c[0] = measure q[0];
        c[1] = measure q[1];
    }?;
    let env = Environment::builder().build()?;
    let mut prepared = env.prepare_structured(bell)?;
    let mut register = env.state_vector(QubitCount::new(2)?)?;
    let result = prepared.run(&mut register, &RunInputs::default())?;
    println!("{:?}", result.outputs);
    drop(register);
    drop(prepared);
    env.close()?;
    Ok(())
}
