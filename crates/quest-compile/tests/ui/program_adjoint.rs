use quest_compile::QuantumRegionBuilder;
fn main(){let program=QuantumRegionBuilder::new(1,0).unwrap().finish().unwrap();let _=program.adjoint();}
