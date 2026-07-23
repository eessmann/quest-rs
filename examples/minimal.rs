use quest_rs::QuESTEnvironment;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let env = QuESTEnvironment::new()?;
    println!("Hello, World!");
    Ok(())
}
