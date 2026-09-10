use quest::{Environment, Shots, circuit};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let bell = circuit! {
        qubit[2] q;
        bit[2] c;
        h q[0];
        cx q[0], q[1];
        c[0] = measure q[0];
        c[1] = measure q[1];
    }?;
    let env = Environment::builder().build()?;
    let mut prepared = env.prepare(bell)?;
    let counts = prepared.sample_zeroed(Shots::new(1024)?, &[2026, 9, 10])?;
    println!("{:?}", counts.counts);
    drop(prepared);
    env.close()?;
    Ok(())
}
