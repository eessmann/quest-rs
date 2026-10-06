//! Checked native matrix deployment and conservative simultaneous storage evidence.
//!
//! Counts use `QuEST`'s signed 64-bit indices; MPI byte frames use signed 32-bit
//! counts. Budgets include caller-declared concurrent resources and every rank
//! sharing a node. This is capacity evidence, not a performance certificate.
use crate::{Error, MemoryBudget, Result};
use std::ops::Range;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MatrixKind {
	/// Dense native matrices are replicated on every process.
	CompMatr,
	/// Target diagonal matrices are replicated on every process.
	DiagMatr,
	/// Full-register diagonal matrices may have a distributed deployment.
	FullStateDiagMatr,
}

#[derive(Clone, Copy, Debug)]
pub struct MatrixRequest {
	pub kind: MatrixKind,
	pub matrix_qubits: usize,
	pub register_qubits: usize,
	pub density: bool,
	pub ranks: usize,
	/// Explicit placement bound, since MPI rank count does not identify hosts.
	pub ranks_per_node: usize,
	pub gpu: bool,
	pub distributed_diagonal: bool,
	/// Other simultaneously live state, workspace and descriptor reservations.
	pub concurrent_bytes: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MatrixAdmission {
	pub replicated: bool,
	pub local_matrix_elements: usize,
	/// Complete diagonal required by distributed density execution.
	pub gather_elements: usize,
	pub local_register_elements: usize,
	pub peak_rank_bytes: usize,
	pub peak_node_bytes: usize,
	pub native_index_max: u64,
	pub mpi_count_max: u32,
}

pub(crate) fn power(qubits: usize) -> Result<usize> {
	let dimension = 1usize
		.checked_shl(u32::try_from(qubits).map_err(|_| Error::Overflow)?)
		.ok_or(Error::Overflow)?;
	i64::try_from(dimension).map_err(|_| Error::Overflow)?;
	Ok(dimension)
}
pub(crate) fn matrix_elements(kind: MatrixKind, qubits: usize) -> Result<usize> {
	let dimension = power(qubits)?;
	let elements = if kind == MatrixKind::CompMatr {
		dimension.checked_mul(dimension).ok_or(Error::Overflow)?
	} else {
		dimension
	};
	i64::try_from(elements).map_err(|_| Error::Overflow)?;
	Ok(elements)
}
pub(crate) fn admit_dense_partition(local: usize, targets: usize) -> Result<()> {
	if power(targets)? > local {
		return Err(Error::Unsupported(
			"distributed communication buffer cannot hold the gate's mixed amplitudes",
		));
	}
	Ok(())
}
impl MatrixRequest {
	/// Admit native index limits, deployment support and simultaneous rank/node peaks.
	/// Storage allows both forward/adjoint matrices plus conservative staging and
	/// native metadata, using the existing prepared-matrix copy allowance.
	/// # Errors
	/// Rejects invalid placement, unsupported deployment, overflow and either budget.
	pub fn admit(
		self,
		rank_budget: MemoryBudget,
		node_budget: MemoryBudget,
	) -> Result<MatrixAdmission> {
		if self.register_qubits == 0
			|| self.matrix_qubits == 0
			|| self.matrix_qubits > self.register_qubits
			|| !self.ranks.is_power_of_two()
			|| self.ranks_per_node == 0
			|| self.ranks_per_node > self.ranks
			|| (self.distributed_diagonal
				&& (self.kind != MatrixKind::FullStateDiagMatr || self.ranks == 1))
			|| (self.kind == MatrixKind::FullStateDiagMatr
				&& self.matrix_qubits != self.register_qubits)
		{
			return Err(Error::Value("invalid native matrix deployment"));
		}
		// Native register creation limits communicator size by physical qubits,
		// including density matrices whose storage has twice as many index bits.
		if self.ranks > power(self.register_qubits)? {
			return Err(Error::Unsupported(
				"native rank count exceeds physical register dimension",
			));
		}
		let width = self
			.register_qubits
			.checked_mul(if self.density { 2 } else { 1 })
			.ok_or(Error::Overflow)?;
		let register_elements = power(width)?;
		if self.ranks > register_elements {
			return Err(Error::Value("native register partition is empty"));
		}
		let local_register_elements = register_elements
			.checked_div(self.ranks)
			.ok_or(Error::Overflow)?;
		if self.kind == MatrixKind::CompMatr {
			admit_dense_partition(local_register_elements, self.matrix_qubits)?;
		}
		let elements = matrix_elements(self.kind, self.matrix_qubits)?;
		let local_matrix_elements = if self.distributed_diagonal {
			if self.ranks > elements {
				return Err(Error::Value("native diagonal partition is empty"));
			}
			elements.checked_div(self.ranks).ok_or(Error::Overflow)?
		} else {
			elements
		};
		let gather_elements = if self.density && self.distributed_diagonal {
			elements
		} else {
			0
		};
		if gather_elements > local_register_elements {
			return Err(Error::Unsupported(
				"native density partition cannot gather full diagonal",
			));
		}
		let matrix_bytes =
			crate::values::bytes_for(local_matrix_elements, if self.gpu { 16 } else { 12 })?;
		let gather_bytes = crate::values::bytes_for(gather_elements, if self.gpu { 2 } else { 1 })?;
		let peak_rank_bytes = self
			.concurrent_bytes
			.checked_add(matrix_bytes)
			.and_then(|n| n.checked_add(gather_bytes))
			.and_then(|n| n.checked_add(4096))
			.ok_or(Error::Overflow)?;
		let peak_node_bytes = peak_rank_bytes
			.checked_mul(self.ranks_per_node)
			.ok_or(Error::Overflow)?;
		for (requested, available) in [
			(peak_rank_bytes, rank_budget.bytes()),
			(peak_node_bytes, node_budget.bytes()),
		] {
			if requested > available {
				return Err(Error::Budget {
					requested,
					available,
				});
			}
		}
		Ok(MatrixAdmission {
			replicated: !self.distributed_diagonal,
			local_matrix_elements,
			gather_elements,
			local_register_elements,
			peak_rank_bytes,
			peak_node_bytes,
			native_index_max: u64::try_from(i64::MAX).map_err(|_| Error::Overflow)?,
			mpi_count_max: u32::try_from(i32::MAX).map_err(|_| Error::Overflow)?,
		})
	}
}

/// Checked deterministic byte-count chunking, without allocating a count table.
#[derive(Clone, Debug)]
pub struct CountChunks {
	next: usize,
	total: usize,
	elements_per_chunk: usize,
}
impl CountChunks {
	/// # Errors
	/// Rejects zero or oversized element widths and native signed-index overflow.
	pub fn new(total: usize, element_bytes: usize) -> Result<Self> {
		i64::try_from(total).map_err(|_| Error::Overflow)?;
		let elements_per_chunk = usize::try_from(i32::MAX)
			.map_err(|_| Error::Overflow)?
			.checked_div(element_bytes)
			.filter(|&n| n != 0)
			.ok_or(Error::Value("invalid MPI frame element width"))?;
		Ok(Self {
			next: 0,
			total,
			elements_per_chunk,
		})
	}
}
impl Iterator for CountChunks {
	type Item = Range<usize>;
	fn next(&mut self) -> Option<Self::Item> {
		if self.next == self.total {
			return None;
		}
		let start = self.next;
		self.next = start
			.saturating_add(self.elements_per_chunk)
			.min(self.total);
		Some(start..self.next)
	}
}
