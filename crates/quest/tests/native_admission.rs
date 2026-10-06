use quest::{
	MemoryBudget,
	native_admission::{MatrixKind, MatrixRequest},
};

#[test]
fn dense_partition_limit_and_replicated_storage_are_independent() {
	let request = MatrixRequest {
		kind: MatrixKind::CompMatr,
		matrix_qubits: 3,
		register_qubits: 4,
		density: false,
		ranks: 4,
		ranks_per_node: 2,
		gpu: false,
		distributed_diagonal: false,
		concurrent_bytes: 100,
	};
	assert!(
		request
			.admit(MemoryBudget::new(1 << 20), MemoryBudget::new(1 << 21))
			.is_err()
	);
	let accepted = MatrixRequest {
		matrix_qubits: 2,
		..request
	}
	.admit(MemoryBudget::new(1 << 20), MemoryBudget::new(1 << 21))
	.unwrap();
	assert_eq!(accepted.local_matrix_elements, 16);
	assert_eq!(accepted.peak_node_bytes, accepted.peak_rank_bytes * 2);
	assert!(accepted.replicated);
	assert!(
		MatrixRequest {
			concurrent_bytes: 1 << 20,
			matrix_qubits: 2,
			..request
		}
		.admit(MemoryBudget::new(1 << 20), MemoryBudget::new(1 << 22))
		.is_err()
	);
}

#[test]
fn distributed_full_diagonal_density_includes_gather_peak() {
	let request = MatrixRequest {
		kind: MatrixKind::FullStateDiagMatr,
		matrix_qubits: 5,
		register_qubits: 5,
		density: false,
		ranks: 4,
		ranks_per_node: 4,
		gpu: false,
		distributed_diagonal: true,
		concurrent_bytes: 0,
	};
	let sv = request
		.admit(MemoryBudget::new(100_000), MemoryBudget::new(400_000))
		.unwrap();
	let dm = MatrixRequest {
		density: true,
		..request
	}
	.admit(MemoryBudget::new(100_000), MemoryBudget::new(400_000))
	.unwrap();
	assert_eq!(sv.local_matrix_elements, 8);
	assert_eq!(dm.gather_elements, 32);
	assert_eq!(sv.gather_elements, 0);
	assert!(dm.peak_rank_bytes > sv.peak_rank_bytes);
	assert!(
		MatrixRequest {
			register_qubits: 32,
			matrix_qubits: 32,
			density: true,
			..request
		}
		.admit(MemoryBudget::new(usize::MAX), MemoryBudget::new(usize::MAX))
		.is_err()
	);
}

#[test]
fn count_chunks_preserve_whole_range_without_native_count_overflow() {
	let count = usize::try_from(i32::MAX).unwrap() + 17;
	let chunks: Vec<_> = quest::native_admission::CountChunks::new(count, 16)
		.unwrap()
		.collect();
	assert_eq!(chunks.len(), 17);
	assert_eq!(chunks.first().unwrap().start, 0);
	assert_eq!(chunks.last().unwrap().end, count);
	assert!(chunks.windows(2).all(|pair| pair[0].end == pair[1].start));
	assert!(
		chunks
			.iter()
			.all(|chunk| i32::try_from((chunk.end - chunk.start) * 16).is_ok())
	);
	assert!(quest::native_admission::CountChunks::new(1, 0).is_err());
}

#[test]
fn density_native_rank_limit_uses_physical_qubits() {
	for ranks in [8, 16] {
		let request = MatrixRequest {
			kind: MatrixKind::DiagMatr,
			matrix_qubits: 1,
			register_qubits: 2,
			density: true,
			ranks,
			ranks_per_node: 1,
			gpu: false,
			distributed_diagonal: false,
			concurrent_bytes: 0,
		};
		assert!(
			request
				.admit(MemoryBudget::new(100_000), MemoryBudget::new(100_000))
				.is_err()
		);
	}
}
