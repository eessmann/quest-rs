//! Independent bridge/deployment witness; run each mode in a fresh process.
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mode = std::env::args().nth(1).ok_or("supply cpu, omp or gpu")?;
    let kind = std::env::args().nth(2).ok_or("supply sv or dm")?;
    let (gpu, threads) = match mode.as_str() {
        "cpu" => (false, false),
        "omp" => (false, true),
        "gpu" => (true, false),
        _ => return Err("supply cpu, omp or gpu".into()),
    };
    let density = match kind.as_str() {
        "sv" => false,
        "dm" => true,
        _ => return Err("supply sv or dm".into()),
    };
    quest_sys::init_custom_quest_env(false, gpu, threads)?;
    let mut register = quest_sys::create_custom_qureg(
        4,
        i32::from(density),
        0,
        i32::from(gpu),
        i32::from(threads),
    )?;
    let deployment = quest_sys::get_qureg_deployment(&register)?;
    assert_eq!(deployment.is_density_matrix, i32::from(density));
    assert_eq!(deployment.is_gpu_accelerated, i32::from(gpu));
    assert_eq!(deployment.is_multithreaded, i32::from(threads));
    assert_eq!(deployment.is_distributed, 0);
    assert_eq!(deployment.num_qubits, 4);
    assert_eq!(deployment.num_nodes, 1);
    assert_eq!(deployment.rank, 0);
    assert_eq!(deployment.num_amps_per_node, if density { 256 } else { 16 });
    quest_sys::init_zero_state(register.pin_mut())?;
    quest_sys::apply_hadamard(register.pin_mut(), 0)?;
    quest_sys::apply_controlled_pauli_x(register.pin_mut(), 0, 1)?;
    quest_sys::apply_phase_shift(register.pin_mut(), 1, 0.37)?;
    quest_sys::sync_quest_env()?;
    let amplitude = |index| {
        let scale = std::f64::consts::FRAC_1_SQRT_2;
        match index {
            0 => (scale, 0.0),
            3 => (scale * 0.37_f64.cos(), scale * 0.37_f64.sin()),
            _ => (0.0, 0.0),
        }
    };
    let mut max_error = 0.0_f64;
    for row in 0..16 {
        for col in 0..if density { 16 } else { 1 } {
            let (ar, ai) = amplitude(row);
            let (re, im, actual) = if density {
                let (br, bi) = amplitude(col);
                (
                    ar * br + ai * bi,
                    ai * br - ar * bi,
                    quest_sys::get_density_qureg_amp(&register, row, col)?,
                )
            } else {
                (ar, ai, quest_sys::get_qureg_amp(&register, row)?)
            };
            max_error = max_error.max((actual.re - re).abs());
            max_error = max_error.max((actual.im - im).abs());
        }
    }
    assert!(max_error < 1e-12);
    let probability = quest_sys::calc_total_prob(&register)?;
    assert!((probability - 1.0).abs() < 1e-12);
    println!(
        "{{\"mode\":\"{mode}\",\"kind\":\"{kind}\",\"status\":\"complete\",\"complex_entries\":{},\"max_error\":{max_error},\"probability\":{probability}}}",
        deployment.num_amps_per_node
    );
    drop(register);
    quest_sys::finalize_quest_env()?;
    Ok(())
}
