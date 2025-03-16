use quest_sys::*;

fn main() {
    // Initialize the QuEST environment
    println!("Initializing QuEST environment...");
    initQuESTEnv();

    // Create a 2-qubit quantum register
    println!("Creating a 2-qubit quantum register...");
    let mut qureg = createQureg(2);

    // Initialize to |00⟩ state
    println!("Initializing to |00⟩ state...");
    initZeroState(qureg.pin_mut());

    // Display initial state properties
    let init_prob = calcTotalProb(&qureg);
    println!("Initial state probability: {}", init_prob);

    let amp00 = getQuregAmp(qureg.pin_mut(), 0);
    println!("Initial amplitude of |00⟩: {} + {}i", amp00.re, amp00.im);

    // Apply Hadamard to the first qubit to create superposition: (|0⟩ + |1⟩)/√2 ⊗ |0⟩
    println!("\nApplying Hadamard to qubit 0...");
    applyHadamard(qureg.pin_mut(), 0);

    // Check amplitudes after Hadamard
    let amp00 = getQuregAmp(qureg.pin_mut(), 0);  // |00⟩
    let amp10 = getQuregAmp(qureg.pin_mut(), 2);  // |10⟩

    println!("Amplitude of |00⟩: {} + {}i", amp00.re, amp00.im);
    println!("Amplitude of |10⟩: {} + {}i", amp10.re, amp10.im);

    // Apply CNOT gate (controlled-X) where qubit 0 controls qubit 1
    // This creates the Bell state: (|00⟩ + |11⟩)/√2
    println!("\nApplying CNOT with qubit 0 controlling qubit 1...");
    applyControlledPauliX(qureg.pin_mut(), 0, 1);

    // Check Bell state amplitudes
    let amp00 = getQuregAmp(qureg.pin_mut(), 0);  // |00⟩
    let amp11 = getQuregAmp(qureg.pin_mut(), 3);  // |11⟩

    println!("Amplitude of |00⟩: {} + {}i", amp00.re, amp00.im);
    println!("Amplitude of |11⟩: {} + {}i", amp11.re, amp11.im);

    // Calculate measurement probabilities
    let prob_q0_0 = calcProbOfQubitOutcome(&qureg, 0, 0);
    let prob_q0_1 = calcProbOfQubitOutcome(&qureg, 0, 1);
    let prob_q1_0 = calcProbOfQubitOutcome(&qureg, 1, 0);
    let prob_q1_1 = calcProbOfQubitOutcome(&qureg, 1, 1);

    println!("\nMeasurement probabilities:");
    println!("Prob of qubit 0 = 0: {}", prob_q0_0);
    println!("Prob of qubit 0 = 1: {}", prob_q0_1);
    println!("Prob of qubit 1 = 0: {}", prob_q1_0);
    println!("Prob of qubit 1 = 1: {}", prob_q1_1);

    // Measure qubit 0
    println!("\nMeasuring qubit 0...");
    let outcome = applyQubitMeasurement(qureg.pin_mut(), 0);
    println!("Measurement outcome: {}", outcome);

    // After measurement, qubit 0 collapses to |outcome⟩, and qubit 1 should be in the same state
    // due to entanglement. Let's verify this:
    let prob_q1_outcome = calcProbOfQubitOutcome(&qureg, 1, outcome);
    println!("Probability of qubit 1 = {}: {}", outcome, prob_q1_outcome);

    // Clean up resources
    println!("\nCleaning up resources...");
    destroyQureg(qureg.pin_mut());
    finalizeQuESTEnv();

    println!("Example completed successfully!");
}