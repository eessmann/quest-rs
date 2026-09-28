//! Run as an extra bin in the independently generated direct consumer fixture.
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mode = std::env::args().nth(1).ok_or("supply cpu, omp or gpu")?;
    let (gpu, threads) = match mode.as_str() {
        "cpu" => (false, false),
        "omp" => (false, true),
        "gpu" => (true, false),
        _ => return Err("supply cpu, omp or gpu".into()),
    };
    quest_sys::init_custom_quest_env(false, gpu, threads)?;
    let environment = quest_sys::get_quest_env()?;
    assert_eq!(environment.is_gpu_accelerated, gpu);
    assert_eq!(environment.is_multithreaded, threads);
    // Explicit per-register flags require this backend, independent of the
    // automatic deployment thresholds used by create_qureg.
    let mut register = quest_sys::create_custom_qureg(4, 0, 0, i32::from(gpu), i32::from(threads))?;
    quest_sys::init_zero_state(register.pin_mut())?;
    quest_sys::apply_hadamard(register.pin_mut(), 0)?;
    quest_sys::apply_controlled_pauli_x(register.pin_mut(), 0, 1)?;
    quest_sys::apply_phase_shift(register.pin_mut(), 1, 0.37)?;
    let scale = std::f64::consts::FRAC_1_SQRT_2;
    for index in 0..16 {
        let value = quest_sys::get_qureg_amp(&register, index)?;
        let (re, im) = match index {
            0 => (scale, 0.0),
            3 => (scale * 0.37_f64.cos(), scale * 0.37_f64.sin()),
            _ => (0.0, 0.0),
        };
        assert!((value.re - re).abs() < 1e-12);
        assert!((value.im - im).abs() < 1e-12);
    }
    let probability = quest_sys::calc_total_prob(&register)?;
    assert!((probability - 1.0).abs() < 1e-12);
    println!("mode={mode}; probability={probability}; every complex amplitude checked");
    quest_sys::report_qureg(&register)?;
    drop(register);
    quest_sys::finalize_quest_env()?;
    Ok(())
}
