//! Executable QSP/numerical book companion, requiring no native `QuEST` installation.
// ANCHOR: numerical_prelude
use quest_polynomial::{Chebyshev, Interval, Laurent, Limits, Polynomial, RemezBuilder, function};
use quest_qsp::{Complex64, FrozenCandidate, RealParityWx, SynthesisBuilder, UnitCircleResponse};
pub type TutorialResult<T> = Result<T, Box<dyn std::error::Error>>;
// ANCHOR_END: numerical_prelude

// ANCHOR: polynomial_interval
/// # Errors
/// Reports invalid coefficients, domains, or interval arithmetic.
pub fn polynomial_and_interval() -> TutorialResult<f64> {
    // p(x) = 0.1 + T_2(x) = 2*x*x - 0.9.
    let polynomial = Polynomial::new(
        Chebyshev,
        vec![
            Complex64::new(0.1, 0.0),
            Complex64::new(0.0, 0.0),
            Complex64::new(1.0, 0.0),
        ],
        Limits::default(),
    )?;
    let value = polynomial.evaluate_real(0.3)?;
    let enclosure = polynomial.evaluate_interval(Interval::new(0.29, 0.31)?)?;
    if value < enclosure.lower() || value > enclosure.upper() {
        return Err("the scalar example escaped its interval enclosure".into());
    }
    let derivative = polynomial.derivative()?;
    let _derivative_value = derivative.evaluate_real(0.3)?;
    Ok(value)
}
// ANCHOR_END: polynomial_interval

// ANCHOR: function_remez
/// # Errors
/// Reports expression domains, resource limits, or an unestablished approximation.
pub fn function_and_remez() -> TutorialResult<f64> {
    let target = function!(|x| x.exp());
    let value_and_derivatives = target.jet(0.3)?;
    let interval_jet = target.jet_interval(Interval::new(-1.0, 1.0)?)?;
    let _ = (value_and_derivatives, interval_jet);
    let approximation = RemezBuilder::new()
        .target(target)
        .degree(3)
        .tolerance(1e-8)
        .domain(Interval::new(-1.0, 1.0)?)?
        .run()?;
    // Tolerance bounds the gap to the minimax lower bound, not the total error.
    let uniform_error = approximation.error_bound().upper();
    let minimax_lower = approximation.minimax_lower_bound();
    if uniform_error - minimax_lower > 1e-8 {
        return Err("requested minimax gap was not established".into());
    }
    Ok(uniform_error)
}
// ANCHOR_END: function_remez

// ANCHOR: canonical_synthesis
/// # Errors
/// Reports invalid parity, insufficient contractivity, budgets or failed numerics.
pub fn canonical_synthesis() -> TutorialResult<FrozenCandidate<RealParityWx>> {
    let target = Polynomial::new(
        Chebyshev,
        vec![Complex64::new(0.0, 0.0), Complex64::new(0.6, 0.0)],
        Limits::default(),
    )?;
    let admitted = SynthesisBuilder::new().real_parity_wx(&target)?.admit()?;
    let completed = admitted.complete()?;
    let frozen = completed.synthesize()?;
    // These are immutable binary64 Wx phases for p(x)=0.6*x.
    let _phases = frozen.phases();
    Ok(frozen)
}
// ANCHOR_END: canonical_synthesis

// ANCHOR: generalized_synthesis
/// # Errors
/// Reports invalid support, insufficient contractivity, budgets or failed numerics.
pub fn generalized_synthesis() -> TutorialResult<FrozenCandidate<UnitCircleResponse>> {
    let target = Polynomial::new(
        Laurent::new(0),
        vec![Complex64::new(0.1, 0.2), Complex64::new(-0.3, 0.1)],
        Limits::default(),
    )?;
    let frozen = SynthesisBuilder::new()
        .unit_circle_response(&target)?
        .admit()?
        .complete()?
        .synthesize()?;
    // The exported matrices execute C0 D(z) C1, with D(z)=diag(z,1).
    let _controls = frozen.controls();
    Ok(frozen)
}
// ANCHOR_END: generalized_synthesis

// ANCHOR: independent_certification
#[cfg(feature = "certification")]
/// # Errors
/// Reports synthesis failure, budgets, a demonstrated violation or an insufficient bound.
pub fn independent_certification() -> TutorialResult<f64> {
    use quest_qsp::certification::{CertificationBuilder, CertificationPolicy};
    let frozen = canonical_synthesis()?;
    let certified = CertificationBuilder::new()
        .candidate(frozen)
        .policy(CertificationPolicy::default())?
        .certify()?;
    // Each bound retains its Astro Float endpoints. This summary is rounded upward.
    let response_bound = certified.report().response().upper_f64();
    let _unitarity_bound = certified.report().unitarity();
    let _all_four_coefficient_arrays = certified.report().coefficients();
    Ok(response_bound)
}
// ANCHOR_END: independent_certification

// ANCHOR: offline_synthesis
#[cfg(feature = "offline-synthesis")]
/// # Errors
/// Reports original-domain, resource, precision, export or certification failure.
pub fn explicit_offline_synthesis() -> TutorialResult<f64> {
    use quest_qsp::offline::{OfflineBuilder, OfflinePolicy};
    let original = Polynomial::new(
        Laurent::new(0),
        vec![Complex64::new(0.3, 0.4)],
        Limits::default(),
    )?;
    let solved = OfflineBuilder::new()
        .unit_circle_response(&original)?
        .policy(OfflinePolicy::default())?
        .solve()?;
    // Every retry starts from original coefficients. Computation and certification
    // durations remain separate in the attempt report.
    let _attempts = solved.report().attempts();
    Ok(solved.certified().report().response().upper_f64())
}
// ANCHOR_END: offline_synthesis

// ANCHOR: offline_approximation
#[cfg(feature = "offline-synthesis")]
/// # Errors
/// Reports undefined functions, exceeded budgets or an unestablished error enclosure.
pub fn explicit_offline_approximation() -> TutorialResult<f64> {
    use quest_qsp::offline::{OfflineRemezBuilder, OfflineRemezPolicy};
    let approximation = OfflineRemezBuilder::new()
        .function(function!(|x| x.exp()))
        .domain(Interval::new(-1.0, 1.0)?)?
        .degree(3)
        .policy(OfflineRemezPolicy {
            error_tolerance: 0.006,
            ..OfflineRemezPolicy::default()
        })?
        .solve()?;
    // This is a rigorous uniform-error bound for the exported polynomial.
    // The separately reported Astro Float exchange gap is empirical.
    Ok(approximation.error_bound().upper())
}
// ANCHOR_END: offline_approximation

// ANCHOR: stage_observation
/// # Errors
/// Preserves errors from the observed operation.
pub fn stage_observation() -> TutorialResult<usize> {
    use quest_numerics::observer::{MonotonicClock, Stage, TraceObserver, observe_result};
    let clock = MonotonicClock::new();
    let mut trace = TraceObserver::new(16);
    let _frozen = observe_result(&mut trace, &clock, Stage::Synthesis, canonical_synthesis)?;
    // Observe the whole certify() call in the same way to include all retries.
    Ok(trace.events().len())
}
// ANCHOR_END: stage_observation

#[cfg(not(test))]
fn main() -> TutorialResult<()> {
    println!("polynomial value: {}", polynomial_and_interval()?);
    println!("exp approximation bound: {}", function_and_remez()?);
    println!(
        "real_parity_wx response at 0.3: {}",
        canonical_synthesis()?.response(0.3)?
    );
    println!(
        "unit_circle_response matrices: {}",
        generalized_synthesis()?.controls().len()
    );
    println!("recorded stages: {}", stage_observation()?);
    #[cfg(feature = "certification")]
    println!("certified response bound: {}", independent_certification()?);
    #[cfg(feature = "offline-synthesis")]
    {
        println!(
            "offline certified response bound: {}",
            explicit_offline_synthesis()?
        );
        println!(
            "offline approximation bound: {}",
            explicit_offline_approximation()?
        );
    }
    Ok(())
}

// ANCHOR: parallel_synthesis
#[cfg(feature = "rayon")]
/// # Errors
/// Reports pool construction, target admission or numerical stage failures.
pub fn parallel_synthesis() -> TutorialResult<FrozenCandidate<RealParityWx>> {
    use quest_numerics::ExecutionPolicy;
    let target = Polynomial::new(
        Chebyshev,
        vec![Complex64::new(0.0, 0.0), Complex64::new(0.6, 0.0)],
        Limits::default(),
    )?;
    let pool = rayon::ThreadPoolBuilder::new().num_threads(4).build()?;
    let execution = ExecutionPolicy::Rayon(&pool);
    let candidate = SynthesisBuilder::new()
        .real_parity_wx(&target)?
        .admit()?
        .complete_with(execution)?
        .synthesize_with(execution)?;
    // The candidate owns its phases and does not borrow the pool.
    drop(pool);
    Ok(candidate)
}
// ANCHOR_END: parallel_synthesis
