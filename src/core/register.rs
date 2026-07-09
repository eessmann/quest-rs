use cxx::UniquePtr;
use ndarray::Array1;
use num::complex::Complex64;
use quest_sys::{QuestResult, Qureg};

pub struct QuantumRegister {
    qureg: UniquePtr<Qureg>,
    num_qubits: usize,
    is_density_matrix: bool,
}

impl QuantumRegister {
    pub fn new(num_qubits: usize) -> QuestResult<Self> {
        Ok(Self {
            qureg: quest_sys::create_qureg(num_qubits as i32)?,
            num_qubits,
            is_density_matrix: false,
        })
    }

    pub fn new_density(num_qubits: usize) -> QuestResult<Self> {
        Ok(Self {
            qureg: quest_sys::create_density_qureg(num_qubits as i32)?,
            num_qubits,
            is_density_matrix: true,
        })
    }

    // State initialization methods
    pub fn init_zero(&mut self) -> QuestResult<()> {
        quest_sys::init_zero_state(self.qureg.pin_mut())
    }

    pub fn init_plus(&mut self) -> QuestResult<()> {
        quest_sys::init_plus_state(self.qureg.pin_mut())
    }

    // Measurement
    pub fn measure_qubit(&mut self, qubit: usize) -> QuestResult<bool> {
        let result = quest_sys::apply_qubit_measurement(self.qureg.pin_mut(), qubit as i32)?;
        Ok(result == 1)
    }

    // Integration with ndarray
    pub fn to_statevector(&self) -> QuestResult<Array1<Complex64>> {
        // Get amplitudes and convert to ndarray
        let amps = quest_sys::get_qureg_amps(&self.qureg, 0, 1_i64 << self.num_qubits as i64)?;

        let dim = 1 << self.num_qubits;
        let mut result = Array1::zeros(dim);

        for (i, amp) in amps.iter().enumerate() {
            result[i] = Complex64::new(amp.re, amp.im);
        }

        Ok(result)
    }

    pub fn is_density_matrix(&self) -> bool {
        self.is_density_matrix
    }
}
