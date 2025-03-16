use quest_sys::Qureg;
use cxx::UniquePtr;
use ndarray::{Array1, Array2, ArrayView1};
use num_complex::Complex64;

pub struct QuantumRegister {
    qureg: UniquePtr<Qureg>,
    num_qubits: usize,
    is_density_matrix: bool,
}

impl QuantumRegister {
    pub fn new(num_qubits: usize) -> Self {
        Self {
            qureg: quest_sys::createQureg(num_qubits as i32),
            num_qubits,
            is_density_matrix: false,
        }
    }

    pub fn new_density(num_qubits: usize) -> Self {
        Self {
            qureg: quest_sys::createDensityQureg(num_qubits as i32),
            num_qubits,
            is_density_matrix: true,
        }
    }

    // State initialization methods
    pub fn init_zero(&mut self) {
        quest_sys::initZeroState(self.qureg.pin_mut());
    }

    pub fn init_plus(&mut self) {
        quest_sys::initPlusState(self.qureg.pin_mut());
    }

    // Measurement
    pub fn measure_qubit(&mut self, qubit: usize) -> bool {
        let result = quest_sys::applyQubitMeasurement(self.qureg.pin_mut(), qubit as i32);
        result == 1
    }

    // Integration with ndarray
    pub fn to_statevector(&mut self) -> Array1<Complex64> {
        // Get amplitudes and convert to ndarray
        let amps = quest_sys::getQuregAmps(
            self.qureg.pin_mut(),
            0,
            (1_i64 << self.num_qubits as i64)
        );

        let dim = 1 << self.num_qubits;
        let mut result = Array1::zeros(dim);

        for (i, amp) in amps.iter().enumerate() {
            result[i] = Complex64::new(amp.re, amp.im);
        }

        result
    }
}

impl Drop for QuantumRegister {
    fn drop(&mut self) {
        quest_sys::destroyQureg(self.qureg.pin_mut());
    }
}