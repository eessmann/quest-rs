//! Fixed persisted sharded weighted inverse consumer; campaign release is external.
#[cfg(all(feature = "qsvt-io", feature = "mpi", quest_native_mpi))]
#[path = "persisted_weighted_transform/policy.rs"]
mod policy;

#[cfg(all(feature = "qsvt-io", feature = "mpi", quest_native_mpi))]
#[path = "persisted_weighted_transform/execution.rs"]
mod execution;
#[cfg(all(feature = "qsvt-io", feature = "mpi", quest_native_mpi))]
#[path = "persisted_weighted_transform/phases.rs"]
mod phases;
#[cfg(all(feature = "qsvt-io", feature = "mpi", quest_native_mpi))]
#[path = "persisted_weighted_transform/runtime.rs"]
mod runtime;
#[cfg(all(feature = "qsvt-io", feature = "mpi", quest_native_mpi))]
fn main() -> Result<(), Box<dyn std::error::Error>> {
	runtime::run()
}
#[cfg(not(all(feature = "qsvt-io", feature = "mpi", quest_native_mpi)))]
fn main() {
	println!(
		r#"{{"schema":"quest-persisted-weighted-transform-unsupported-v1","status":"rejected","error":"requires qsvt-io,mpi and native MPI QuEST"}}"#
	);
	std::process::exit(2);
}

#[cfg(all(test, feature = "qsvt-io", feature = "mpi", quest_native_mpi))]
mod tests {
	use super::policy;
	use quest::Complex64;
	type Result = std::result::Result<(), Box<dyn std::error::Error>>;
	#[test]
	fn source_generation_has_exact_cyclic_owners_and_signed_distinct_terms() -> Result {
		for term in 0..3 {
			let mut all = Vec::new();
			for rank in 0..8 {
				let local = policy::entries(32, term, rank, 8)?;
				let local = local.collect::<quest_numerics::Result<Vec<_>>>()?;
				assert_eq!(local.len(), 8);
				assert!(local.iter().all(|e| e.column % 8 == rank));
				all.extend(local);
			}
			all.sort_by_key(|e| e.ordinal);
			assert_eq!(all.len(), 64);
			for (ordinal, e) in all.iter().enumerate() {
				assert_eq!(e.ordinal, u64::try_from(ordinal)?);
				assert_eq!(e.column, ordinal / 2);
				assert_eq!(e.row, (ordinal / 2) ^ (ordinal % 2));
				assert_eq!(e.value, policy::coefficient(term, ordinal % 2)?);
			}
		}
		assert!(policy::entries(32, 3, 0, 8).is_err());
		assert!(policy::entries(32, 0, 8, 8).is_err());
		assert_eq!(policy::coefficient(2, 0)?, Complex64::new(-1., 0.));
		Ok(())
	}
	#[test]
	fn complete_mapping_and_inverse_arithmetic_keep_every_system_coordinate() -> Result {
		let targets = policy::targets(32)?;
		assert_eq!(targets, vec![0, 9, 8, 7, 6, 5, 1]);
		for j in 0..32 {
			let physical = policy::system_index(j, &targets)?;
			assert_eq!(policy::decode_system(physical, &targets)?, j);
			assert_eq!(policy::system_index(j ^ 1, &targets)?, physical ^ 512);
		}
		for adjoint in [false, true] {
			let x0 = Complex64::new(20. / 13., 0.);
			let x1 = Complex64::new(0., if adjoint { 4. / 13. } else { -4. / 13. });
			assert!((policy::residual_entry(x0, x1, 0, adjoint)?).norm() < 1e-15);
			assert!((policy::residual_entry(x1, x0, 1, adjoint)?).norm() < 1e-15);
		}
		assert!(policy::targets(16).is_err());
		Ok(())
	}
	#[test]
	fn nominal_spectrum_is_enclosed_and_finite_chebyshev_oracle_is_independent() -> Result {
		let (lo, hi) = policy::spectrum()?;
		assert!(lo <= 26_f64.sqrt() / 8. && hi >= 26_f64.sqrt() / 8.);
		assert!(lo < hi);
		assert!(
			(policy::chebyshev(&[0., 0.3, 0., -0.1], 0.7)?
				- (0.3 * 0.7 - 0.1 * (4. * 0.7_f64.powi(3) - 3. * 0.7)))
				.abs()
				< 1e-15
		);
		assert!(policy::chebyshev(&[f64::NAN], 0.7).is_err());
		Ok(())
	}
	#[test]
	fn metadata_scan_rejects_unbounded_parsing_before_allocating_the_value() -> Result {
		assert!(super::runtime::metadata_scan(&vec![b' '; 16_385]).is_err());
		assert!(super::runtime::metadata_scan(&b"[".repeat(17)).is_err());
		assert!(
			super::runtime::metadata_scan(&format!("[{}]", "0,".repeat(2048)).into_bytes())
				.is_err()
		);
		super::runtime::metadata_scan(br#"{"schema":"fixed","values":[1,2]}"#)?;
		Ok(())
	}
	#[test]
	fn typed_protocol_frame_binds_limits_signed_weights_layout_and_parts() -> Result {
		for (dimension, count) in [(4, 8), (32, 10)] {
			for parts in [1, 2, 4, 8] {
				let (frame, length) = super::runtime::protocol_frame(dimension, count, parts)?;
				assert!(length <= 1024);
				assert!(
					frame
						.windows(8)
						.any(|w| w == (-0.125_f64).to_bits().to_le_bytes())
				);
			}
		}
		let (a, alen) = super::runtime::protocol_frame(32, 10, 8)?;
		let (b, blen) = super::runtime::protocol_frame(32, 10, 2)?;
		assert!(alen <= 1024);
		assert_eq!(alen, blen);
		assert_ne!(a, b);
		let (c, _) = super::runtime::protocol_frame(4, 8, 8)?;
		assert_ne!(a, c);
		assert!(
			a.windows(8)
				.any(|w| w == (-0.125_f64).to_bits().to_le_bytes())
		);
		assert!(a.windows(8).any(|w| w == 16_384_u64.to_le_bytes()));
		Ok(())
	}
	#[test]
	fn readout_floor_rejects_bad_partitions_scale_and_limits_before_queries() -> Result {
		assert!(super::execution::readout_floor(128, 8, 1., 1_048_576, 4_000_000).is_ok());
		for (local, parts, scale, bytes, work) in [
			(128, 8, f64::NAN, 1_048_576, 4_000_000),
			(128, 8, 0., 1_048_576, 4_000_000),
			(64, 8, 1., 1_048_576, 4_000_000),
			(128, 8, 1., 1, 4_000_000),
			(128, 8, 1., 1_048_576, 1),
			(usize::MAX, 8, 1., 1_048_576, 4_000_000),
		] {
			assert!(super::execution::readout_floor(local, parts, scale, bytes, work).is_err());
		}
		Ok(())
	}
	#[test]
	fn json_owners_require_whole_live_overlap_before_cloning_or_filling() -> Result {
		let first = serde_json::Value::String(String::with_capacity(400_000));
		let second = serde_json::Value::String(String::with_capacity(400_000));
		assert!(super::runtime::json_payload(&first)? <= 524_288);
		assert!(super::runtime::json_payload(&second)? <= 524_288);
		assert!(
			super::runtime::admit_json_overlap(
				&[&first, &second],
				&[1, 1],
				262_144 + 65_536,
				1_048_576
			)
			.is_err()
		);
		assert!(
			super::runtime::admit_json_overlap(&[&first], &[1], 262_144 + 65_536, 1_048_576)
				.is_ok()
		);
		assert!(
			super::runtime::admit_json_overlap(&[&first], &[usize::MAX], 0, usize::MAX).is_err()
		);
		Ok(())
	}
	#[test]
	fn serialized_row_keeps_stage_result_owners_without_cloning() -> Result {
		let stages = serde_json::json!([1, 2, 3]);
		let stage_pointer = stages.as_array().ok_or("stage array")?.as_ptr();
		let payload = serde_json::json!([4, 5, 6]);
		let payload_pointer = payload.as_array().ok_or("payload array")?.as_ptr();
		let mut row = serde_json::json!({"stages":null,"result":null});
		super::runtime::move_row_owners(&mut row, stages, Some(payload))?;
		assert_eq!(
			row.get("stages")
				.and_then(serde_json::Value::as_array)
				.ok_or("moved stages")?
				.as_ptr(),
			stage_pointer
		);
		assert_eq!(
			row.get("result")
				.and_then(serde_json::Value::as_array)
				.ok_or("moved result")?
				.as_ptr(),
			payload_pointer
		);
		let serialized = serde_json::to_vec(&row)?;
		let decoded: serde_json::Value = serde_json::from_slice(&serialized)?;
		assert_eq!(decoded, row);
		Ok(())
	}
	#[test]
	fn bounded_capacity_rejection_preserves_attempt_and_completed_call_progress() -> Result {
		let value = serde_json::json!({"completed_calls":[{"index":0,"adjoint":false,"status":"accuracy-passed","detail":"discard"}],"attempt_index":1,"attempt_progress":{"initialized":true,"applied":true,"readout_completed":false}});
		let summary = super::runtime::compact_progress(&value)?;
		assert_eq!(summary.get("applied"), Some(&serde_json::Value::Bool(true)));
		assert_eq!(
			summary
				.get("completed_calls")
				.and_then(serde_json::Value::as_array)
				.ok_or("call summary")?
				.len(),
			1
		);
		assert!(super::runtime::json_payload(&summary)? < 16_384);
		Ok(())
	}
}
