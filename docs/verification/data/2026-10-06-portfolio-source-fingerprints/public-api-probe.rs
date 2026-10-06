use quest_qsvt::{Complex64, NumericalPolicy, ReplayEncoding, ShiftRegister, TensorShiftEncoding, StructuredStencilEncoding, EncodingDescriptor};
use quest_qsvt::portfolio::{ArithmeticStencil, ArithmeticStencilTerm, Boundary, StructuredScheme, PortfolioLimits, WeightedLcu, LcuPlan, LcuPlanLimits};
fn shift(q:usize,off:usize)->quest_qsvt::Result<TensorShiftEncoding>{TensorShiftEncoding::new(q,vec![ShiftRegister::new(0,q,off)?],NumericalPolicy::default())}
fn check(name:&str,a:EncodingDescriptor,b:EncodingDescriptor){
 println!("{name}: sourceA={:016x} sourceB={:016x} constructionA={:016x} constructionB={:016x} descriptor_equal={} alphaA={} alphaB={}",a.source_identity,b.source_identity,a.construction_identity,b.construction_identity,a==b,a.normalization,b.normalization);
 assert_eq!(a.source_identity,b.source_identity,"actual systematic source collision");assert_ne!(a.construction_identity,b.construction_identity,"ordered construction still distinguishes U");assert_ne!(a,b,"whole descriptor guard does not collapse in these fixtures");
}
fn main()->quest_qsvt::Result<()>{
 let i=Complex64::new(0.,1.);let ni=Complex64::new(0.,-1.);
 let a=LcuPlan::new(vec![(i,shift(2,0)?.descriptor()?),(ni,shift(2,1)?.descriptor()?)],LcuPlanLimits::default())?;
 let b=LcuPlan::new(vec![(ni,shift(2,0)?.descriptor()?),(i,shift(2,1)?.descriptor()?)],LcuPlanLimits::default())?;check("LcuPlan_iI_minus_iS4",a.descriptor().clone(),b.descriptor().clone());
 let a=WeightedLcu::new(vec![(i,shift(2,0)?),(ni,shift(2,1)?)],PortfolioLimits::default())?;
 let b=WeightedLcu::new(vec![(ni,shift(2,0)?),(i,shift(2,1)?)],PortfolioLimits::default())?;check("WeightedLcu_iI_minus_iS4",a.descriptor()?,b.descriptor()?);
 assert_ne!(i,ni);println!("independent A[0,0]: +i versus -i (offset1 contributes row1, not row0)");
 for scheme in [StructuredScheme::Base,StructuredScheme::Prep]{
  let a=ArithmeticStencil::new(vec![2],vec![ArithmeticStencilTerm{weight:i,offsets:vec![0]},ArithmeticStencilTerm{weight:ni,offsets:vec![2]}],Boundary::Periodic,scheme,PortfolioLimits::default())?;
  let b=ArithmeticStencil::new(vec![2],vec![ArithmeticStencilTerm{weight:ni,offsets:vec![0]},ArithmeticStencilTerm{weight:i,offsets:vec![2]}],Boundary::Periodic,scheme,PortfolioLimits::default())?;check(&format!("ArithmeticStencil_{scheme:?}"),a.descriptor()?,b.descriptor()?);
 }
 let a=StructuredStencilEncoding::new(5,vec![(i,shift(5,12)?),(ni,shift(5,16)?)],NumericalPolicy::default())?;
 let b=StructuredStencilEncoding::new(5,vec![(ni,shift(5,12)?),(i,shift(5,16)?)],NumericalPolicy::default())?;check("StructuredStencil_iS12_minus_iS16",a.descriptor()?,b.descriptor()?);
 println!("independent structured A[12,0]: +i versus -i (offset16 contributes row16)");
 let make=|a,b|TensorShiftEncoding::new(16,vec![ShiftRegister::new(0,8,a)?,ShiftRegister::new(8,8,b)?],NumericalPolicy::default());
 let a=make(1,129)?;let b=make(129,1)?;check("TensorShift_two8bit_registers",a.descriptor()?,b.descriptor()?);
 assert_ne!(1+129*256,129+256);println!("independent permutation image0:33025 versus385; no basis/state enumeration");
 Ok(())
}
