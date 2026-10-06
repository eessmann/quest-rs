use quest_qsvt::{Complex64, EncodingDescriptor, MatchingColumn, MatchingHeader, MatchingShard, NumericalPolicy};
fn main() -> Result<(), Box<dyn std::error::Error>> {
 let positive: Vec<_> = (0..4).map(|source| MatchingColumn { color:0,source,destination:source^1,cosine:1.0,sine:0.0,phase:Complex64::new(0.0,1.0) }).collect();
 let negative: Vec<_> = positive.iter().map(|v| MatchingColumn { phase:Complex64::new(0.0,-1.0),..*v }).collect();
 let a=MatchingShard::summarize_records(&positive)?;
 let b=MatchingShard::summarize_records(&negative)?;
 println!("positive_digest={} negative_digest={} equal={}",a.1,b.1,a==b);
 let header=MatchingHeader {rows:4,cols:4,system_qubits:2,color_qubits:0,num_colors:1,beta:1.0,alpha:1.0,source_identity:42,record_count:a.0,record_digest:a.1};
 let original=MatchingShard::from_parts(header,0,1,positive,NumericalPolicy::default())?;
 let altered=MatchingShard::from_parts(header,0,1,negative,NumericalPolicy::default())?;
 println!("altered_phase_admitted={} identical_descriptors={} original_phase={} altered_phase={}",true,EncodingDescriptor::from_matching_header(original.header())?==EncodingDescriptor::from_matching_header(altered.header())?,original.records()[0].phase,altered.records()[0].phase);
 assert_ne!(a,b,"opposite whole-unitary phases must not systematically collide");
 assert_ne!(original.records()[0].phase,altered.records()[0].phase);
 Ok(())
}
