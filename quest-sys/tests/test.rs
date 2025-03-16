use std::f64::consts::PI;
use std::sync::Once;
use approx::assert_relative_eq;
use quest_sys::*;

// Static initialization and cleanup control
static INIT: Once = Once::new();
static mut QUEST_ENV_INITIALIZED: bool = false;

// Initialize QuEST environment once for all tests
fn ensure_quest_env_initialized() {
    INIT.call_once(|| {
        println!("Initializing QuEST environment for all tests...");
        unsafe {
            initQuESTEnv();
            QUEST_ENV_INITIALIZED = true;

            // Register a handler to finalize QuEST at program exit
            std::panic::catch_unwind(|| {
                libc::atexit(finalize_quest_at_exit);
            }).ok();
        }
    });

    // Check if initialization succeeded
    assert!(unsafe { QUEST_ENV_INITIALIZED }, "QuEST environment failed to initialize");
}

// Function to be called at program exit to clean up QuEST
extern "C" fn finalize_quest_at_exit() {
    println!("Finalizing QuEST environment...");
    unsafe {
        if QUEST_ENV_INITIALIZED {
            finalizeQuESTEnv();
            QUEST_ENV_INITIALIZED = false;
        }
    }
}

#[test]
fn test_init_quest_environment() {
    // Simply check that we can initialize the environment
    ensure_quest_env_initialized();
    assert!(true);
}

#[test]
fn test_create_and_destroy_qureg() {
    ensure_quest_env_initialized();

    // Create and destroy manually to test the specific functionality
    let mut qureg = createQureg(2);
    assert!(!qureg.is_null());

    // Box::pin is used to properly pin CXX opaque types
    destroyQureg(qureg.pin_mut());

    // We've successfully destroyed the qureg if we reach here
    assert!(true);
}

#[test]
fn test_create_and_destroy_density_qureg() {
    ensure_quest_env_initialized();

    let mut qureg = createDensityQureg(2);
    assert!(!qureg.is_null());

    destroyQureg(qureg.pin_mut());

    // We've successfully destroyed the density qureg if we reach here
    assert!(true);
}

#[test]
fn test_init_zero_state() {
    ensure_quest_env_initialized();

    let mut qureg = createQureg(2);

    // Initialize to |00⟩
    initZeroState(qureg.pin_mut());

    // In the |00⟩ state, only the first amplitude should be 1, the rest 0
    let amp = getQuregAmp(qureg.pin_mut(), 0);
    assert_eq!(amp.re, 1.0);
    assert_eq!(amp.im, 0.0);

    // Check other amplitudes are zero
    for i in 1..4 {
        let amp = getQuregAmp(qureg.pin_mut(), i);
        assert_eq!(amp.re, 0.0);
        assert_eq!(amp.im, 0.0);
    }

    destroyQureg(qureg.pin_mut());
}

#[test]
fn test_init_plus_state() {
    ensure_quest_env_initialized();

    let mut qureg = createQureg(1);

    initPlusState(qureg.pin_mut());

    // In the |+⟩ state, both amplitudes should be 1/sqrt(2)
    let expected = 1.0 / 2.0_f64.sqrt();

    let amp0 = getQuregAmp(qureg.pin_mut(), 0);
    let amp1 = getQuregAmp(qureg.pin_mut(), 1);

    assert!((amp0.re - expected).abs() < 1e-10);
    assert_eq!(amp0.im, 0.0);

    assert!((amp1.re - expected).abs() < 1e-10);
    assert_eq!(amp1.im, 0.0);

    destroyQureg(qureg.pin_mut());
}

#[test]
fn test_init_arbitrary_pure_state() {
    ensure_quest_env_initialized();

    let mut qureg = createQureg(2);

    // Create a specific state: (|00⟩ + i|11⟩)/sqrt(2)
    let amps = vec![
        complex(1.0 / 2.0_f64.sqrt(), 0.0),   // |00⟩
        complex(0.0, 0.0),                    // |01⟩
        complex(0.0, 0.0),                    // |10⟩
        complex(0.0, 1.0 / 2.0_f64.sqrt()),   // |11⟩
    ];

    initArbitraryPureState(qureg.pin_mut(), &amps);

    // Check the state was correctly initialized
    let amp0 = getQuregAmp(qureg.pin_mut(), 0);
    let amp3 = getQuregAmp(qureg.pin_mut(), 3);

    assert!((amp0.re - 1.0 / 2.0_f64.sqrt()).abs() < 1e-10);
    assert_relative_eq!(amp0.im, 0.0);

    assert_relative_eq!(amp3.re, 0.0);
    assert!((amp3.im - 1.0 / 2.0_f64.sqrt()).abs() < 1e-10);

    // Verify state normalization
    let total_prob = calcTotalProb(&qureg);
    assert!((total_prob - 1.0).abs() < 1e-10);

    destroyQureg(qureg.pin_mut());
}

#[test]
fn test_pauli_x() {
    ensure_quest_env_initialized();

    let mut qureg = createQureg(1);

    // Init to |0⟩
    initZeroState(qureg.pin_mut());

    // Apply X gate (NOT gate)
    applyPauliX(qureg.pin_mut(), 0);

    // Should now be in |1⟩ state
    let amp0 = getQuregAmp(qureg.pin_mut(), 0);
    let amp1 = getQuregAmp(qureg.pin_mut(), 1);

    assert_relative_eq!(amp0.re, 0.0);
    assert_relative_eq!(amp0.im, 0.0);

    assert_relative_eq!(amp1.re, 1.0);
    assert_relative_eq!(amp1.im, 0.0);

    destroyQureg(qureg.pin_mut());
}

#[test]
fn test_pauli_y() {
    ensure_quest_env_initialized();

    let mut qureg = createQureg(1);

    // Init to |0⟩
    initZeroState(qureg.pin_mut());

    // Apply Y gate
    applyPauliY(qureg.pin_mut(), 0);

    // Should now be in i|1⟩ state
    let amp1 = getQuregAmp(qureg.pin_mut(), 1);

    assert_relative_eq!(amp1.re, 0.0);
    assert_relative_eq!(amp1.im, 1.0);

    destroyQureg(qureg.pin_mut());
}

#[test]
fn test_pauli_z() {
    ensure_quest_env_initialized();

    let mut qureg = createQureg(1);

    // Init to |+⟩ = (|0⟩ + |1⟩)/sqrt(2)
    initPlusState(qureg.pin_mut());

    // Apply Z gate
    applyPauliZ(qureg.pin_mut(), 0);

    // Should now be in (|0⟩ - |1⟩)/sqrt(2) state
    let expected = 1.0 / 2.0_f64.sqrt();

    let amp0 = getQuregAmp(qureg.pin_mut(), 0);
    let amp1 = getQuregAmp(qureg.pin_mut(), 1);

    assert!((amp0.re - expected).abs() < 1e-10);
    assert_relative_eq!(amp0.im, 0.0);

    assert!((amp1.re + expected).abs() < 1e-10); // Note the negative sign
    assert_relative_eq!(amp1.im, 0.0);

    destroyQureg(qureg.pin_mut());
}

#[test]
fn test_hadamard() {
    ensure_quest_env_initialized();

    let mut qureg = createQureg(1);

    // Init to |0⟩
    initZeroState(qureg.pin_mut());

    // Apply Hadamard
    applyHadamard(qureg.pin_mut(), 0);

    // Should now be in |+⟩ = (|0⟩ + |1⟩)/sqrt(2) state
    let expected = 1.0 / 2.0_f64.sqrt();

    let amp0 = getQuregAmp(qureg.pin_mut(), 0);
    let amp1 = getQuregAmp(qureg.pin_mut(), 1);

    assert!((amp0.re - expected).abs() < 1e-10);
    assert_relative_eq!(amp0.im, 0.0);

    assert!((amp1.re - expected).abs() < 1e-10);
    assert_relative_eq!(amp1.im, 0.0);

    // Apply Hadamard again
    applyHadamard(qureg.pin_mut(), 0);

    // Should be back to |0⟩
    let amp0 = getQuregAmp(qureg.pin_mut(), 0);
    let amp1 = getQuregAmp(qureg.pin_mut(), 1);

    assert!((amp0.re - 1.0).abs() < 1e-10);
    assert_relative_eq!(amp0.im, 0.0);

    assert!((amp1.re).abs() < 1e-10);
    assert_relative_eq!(amp1.im, 0.0);

    destroyQureg(qureg.pin_mut());
}

#[test]
fn test_two_qubit_phase_shift() {
    ensure_quest_env_initialized();

    let mut qureg = createQureg(2);

    // Create the state |11⟩
    initZeroState(qureg.pin_mut());
    applyPauliX(qureg.pin_mut(), 0);
    applyPauliX(qureg.pin_mut(), 1);

    // Apply phase shift of PI to |11⟩
    applyTwoQubitPhaseShift(qureg.pin_mut(), 0, 1, PI);

    // Should now be in -|11⟩ state
    let amp = getQuregAmp(qureg.pin_mut(), 3); // 3 corresponds to |11⟩

    assert!((amp.re + 1.0).abs() < 1e-10); // Should be -1
    assert_relative_eq!(amp.im, 0.0);

    destroyQureg(qureg.pin_mut());
}

#[test]
fn test_measurement() {
    ensure_quest_env_initialized();

    let mut qureg = createQureg(1);

    // Init to |0⟩
    initZeroState(qureg.pin_mut());

    // Measure should give 0 with probability 1
    let prob = calcProbOfQubitOutcome(&qureg, 0, 0);
    assert!((prob - 1.0).abs() < 1e-10);

    // Since measurement changes the state, we need to reinitialize
    initZeroState(qureg.pin_mut());
    let outcome = applyQubitMeasurement(qureg.pin_mut(), 0);
    assert_eq!(outcome, 0);

    // Now init to |1⟩
    initZeroState(qureg.pin_mut());
    applyPauliX(qureg.pin_mut(), 0);

    // Measure should give 1 with probability 1
    let prob = calcProbOfQubitOutcome(&qureg, 0, 1);
    assert!((prob - 1.0).abs() < 1e-10);

    initZeroState(qureg.pin_mut());
    applyPauliX(qureg.pin_mut(), 0);
    let outcome = applyQubitMeasurement(qureg.pin_mut(), 0);
    assert_eq!(outcome, 1);

    destroyQureg(qureg.pin_mut());
}

#[test]
fn test_density_matrix() {
    ensure_quest_env_initialized();

    let mut qureg = createDensityQureg(1);

    // Init to |0⟩⟨0|
    initZeroState(qureg.pin_mut());

    // Check density matrix elements
    let rho_00 = getDensityQuregAmp(qureg.pin_mut(), 0, 0);
    let rho_01 = getDensityQuregAmp(qureg.pin_mut(), 0, 1);
    let rho_10 = getDensityQuregAmp(qureg.pin_mut(), 1, 0);
    let rho_11 = getDensityQuregAmp(qureg.pin_mut(), 1, 1);

    assert!((rho_00.re - 1.0).abs() < 1e-10);
    assert_relative_eq!(rho_00.im, 0.0);

    assert_relative_eq!(rho_01.re, 0.0);
    assert_relative_eq!(rho_01.im, 0.0);

    assert_relative_eq!(rho_10.re, 0.0);
    assert_relative_eq!(rho_10.im, 0.0);

    assert_relative_eq!(rho_11.re, 0.0);
    assert_relative_eq!(rho_11.im, 0.0);

    // Apply X gate
    applyPauliX(qureg.pin_mut(), 0);

    // Should now be in |1⟩⟨1|
    let rho_00 = getDensityQuregAmp(qureg.pin_mut(), 0, 0);
    let rho_11 = getDensityQuregAmp(qureg.pin_mut(), 1, 1);

    assert_relative_eq!(rho_00.re, 0.0);
    assert_relative_eq!(rho_00.im, 0.0);

    assert!((rho_11.re - 1.0).abs() < 1e-10);
    assert_relative_eq!(rho_11.im, 0.0);

    destroyQureg(qureg.pin_mut());
}

#[test]
fn test_qureg_cloning() {
    ensure_quest_env_initialized();

    // Create source qureg in a specific state
    let mut src_qureg = createQureg(1);
    initZeroState(src_qureg.pin_mut());
    applyHadamard(src_qureg.pin_mut(), 0);

    // Create target qureg
    let mut tgt_qureg = createQureg(1);

    // Clone src to tgt
    setQuregToClone(tgt_qureg.pin_mut(), &src_qureg);

    // Verify the states are identical
    let fidelity = calcFidelity(&src_qureg, &tgt_qureg);
    assert!((fidelity - 1.0).abs() < 1e-10);

    // Clean up
    destroyQureg(src_qureg.pin_mut());
    destroyQureg(tgt_qureg.pin_mut());
}

#[test]
fn test_utility_functions() {
    ensure_quest_env_initialized();

    // Set validation epsilon
    let epsilon = 1e-8;
    setValidationEpsilon(epsilon);

    // Set max number of significant figures for reporting
    setMaxNumReportedSigFigs(10);

    // These just test that the bindings don't crash
    assert!(true);
}