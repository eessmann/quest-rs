use quest::{MatrixPolicy, NumericalOperator};
fn main() {
    let matrix = faer::Mat::<quest::Complex64>::identity(2,2);
    let operator = NumericalOperator::from_view(matrix.as_ref(), MatrixPolicy::default()).unwrap();
    operator.inverse();
}
