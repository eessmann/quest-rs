use quest_circuit::ProgramBuilder;
fn main(){let program=ProgramBuilder::new(1,0).unwrap().finish().unwrap();let _=program.adjoint();}
