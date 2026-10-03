//! Bounded application wire for already frozen numerical inputs. Binary64 words
//! are copied exactly; decoding never performs synthesis or reads a file.
use super::{FrozenInput, Workflow};
use crate::{Error, Result, TransformRoute};
use faer::{Mat, MatRef};
use num_complex::Complex64 as C;
use quest_qsvt_io::{IoPolicy, hdf5::StoredBlockEncoding};

const MAGIC: &[u8; 8] = b"QSVTCLI2";
fn word(value: usize) -> Result<[u8; 8]> {
	Ok(u64::try_from(value)
		.map_err(|_| Error::Budget("wire integer"))?
		.to_le_bytes())
}
pub fn encode(input: &FrozenInput) -> Result<Vec<u8>> {
	let qsp = quest_qsvt_io::write_qsp_execution_json(&input.qsp)?;
	let evidence = serde_json::to_string(&input.evidence)?;
	let matrices = [
		input.block.u(),
		input.block.pi_left(),
		input.block.pi_right(),
	];
	let count = matrices.iter().try_fold(
		input
			.input
			.len()
			.checked_add(input.reference.len())
			.ok_or(Error::Budget("wire vectors"))?,
		|sum, matrix| {
			sum.checked_add(
				matrix
					.nrows()
					.checked_mul(matrix.ncols())
					.ok_or(Error::Budget("wire matrix"))?,
			)
			.ok_or(Error::Budget("wire matrices"))
		},
	)?;
	let capacity = count
		.checked_mul(16)
		.and_then(|n| n.checked_add(qsp.len()))
		.and_then(|n| n.checked_add(evidence.len()))
		.and_then(|n| n.checked_add(256))
		.ok_or(Error::Budget("wire storage"))?;
	if capacity > IoPolicy::default().max_bytes {
		return Err(Error::Budget("wire storage"));
	}
	let mut bytes = Vec::new();
	bytes
		.try_reserve_exact(capacity)
		.map_err(|_| Error::Budget("wire allocation"))?;
	bytes.extend_from_slice(MAGIC);
	bytes.extend_from_slice(&word(match input.workflow {
		Workflow::Embedded => 0,
		Workflow::Overlap => 1,
	})?);
	bytes.extend_from_slice(&word(route_word(input.route)?)?);
	bytes.extend_from_slice(&input.verification_tolerance.to_le_bytes());
	bytes.extend_from_slice(&input.block.alpha().to_le_bytes());
	for n in input
		.block
		.original_dimensions()
		.into_iter()
		.chain(input.block.padded_dimensions())
	{
		bytes.extend_from_slice(&word(n)?);
	}
	for matrix in matrices {
		write_matrix(&mut bytes, matrix)?;
	}
	for values in [&input.input, &input.reference] {
		bytes.extend_from_slice(&word(values.len())?);
		for z in values {
			bytes.extend_from_slice(&z.re.to_le_bytes());
			bytes.extend_from_slice(&z.im.to_le_bytes());
		}
	}
	for text in [qsp, evidence] {
		bytes.extend_from_slice(&word(text.len())?);
		bytes.extend_from_slice(text.as_bytes());
	}
	Ok(bytes)
}
fn write_matrix(bytes: &mut Vec<u8>, matrix: MatRef<'_, C>) -> Result<()> {
	bytes.extend_from_slice(&word(matrix.nrows())?);
	bytes.extend_from_slice(&word(matrix.ncols())?);
	for row in 0..matrix.nrows() {
		for col in 0..matrix.ncols() {
			let z = matrix[(row, col)];
			bytes.extend_from_slice(&z.re.to_le_bytes());
			bytes.extend_from_slice(&z.im.to_le_bytes());
		}
	}
	Ok(())
}
const fn route_word(route: TransformRoute) -> Result<usize> {
	Ok(match route {
		TransformRoute::Auto => {
			return Err(Error::Input(
				"automatic route must be resolved before freezing",
			));
		}
		TransformRoute::Standard => 0,
		TransformRoute::Direct => 1,
		TransformRoute::HermitianizedFull => 2,
		TransformRoute::HermitianizedEven => 3,
		TransformRoute::HermitianizedOdd => 4,
		TransformRoute::MultiplicationEven => 5,
		TransformRoute::MultiplicationOdd => 6,
	})
}
const fn route(value: usize) -> Result<TransformRoute> {
	match value {
		0 => Ok(TransformRoute::Standard),
		1 => Ok(TransformRoute::Direct),
		2 => Ok(TransformRoute::HermitianizedFull),
		3 => Ok(TransformRoute::HermitianizedEven),
		4 => Ok(TransformRoute::HermitianizedOdd),
		5 => Ok(TransformRoute::MultiplicationEven),
		6 => Ok(TransformRoute::MultiplicationOdd),
		_ => Err(Error::Input("unknown wire route")),
	}
}
struct Reader<'a> {
	bytes: &'a [u8],
	position: usize,
}
impl<'a> Reader<'a> {
	fn take(&mut self, count: usize) -> Result<&'a [u8]> {
		let end = self
			.position
			.checked_add(count)
			.ok_or(Error::Budget("wire position"))?;
		let values = self
			.bytes
			.get(self.position..end)
			.ok_or(Error::Input("truncated application wire"))?;
		self.position = end;
		Ok(values)
	}
	fn integer(&mut self) -> Result<usize> {
		let word: [u8; 8] = self
			.take(8)?
			.try_into()
			.map_err(|_| Error::Input("wire integer"))?;
		usize::try_from(u64::from_le_bytes(word)).map_err(|_| Error::Budget("wire integer"))
	}
	fn real(&mut self) -> Result<f64> {
		let word: [u8; 8] = self
			.take(8)?
			.try_into()
			.map_err(|_| Error::Input("wire real"))?;
		let value = f64::from_le_bytes(word);
		if !value.is_finite() {
			return Err(Error::Input("nonfinite application wire"));
		}
		Ok(value)
	}
	fn complex(&mut self) -> Result<C> {
		Ok(C::new(self.real()?, self.real()?))
	}
	fn check_elements(&self, count: usize) -> Result<()> {
		let bytes = count
			.checked_mul(16)
			.ok_or(Error::Budget("wire elements"))?;
		if bytes > IoPolicy::default().max_bytes
			|| bytes > self.bytes.len().saturating_sub(self.position)
		{
			return Err(Error::Budget("wire elements"));
		}
		Ok(())
	}
	fn matrix(&mut self) -> Result<Mat<C>> {
		let rows = self.integer()?;
		let cols = self.integer()?;
		self.check_elements(rows.checked_mul(cols).ok_or(Error::Budget("wire matrix"))?)?;
		let mut matrix = crate::execution::matrix(rows, cols)?;
		for row in 0..rows {
			for col in 0..cols {
				matrix[(row, col)] = self.complex()?;
			}
		}
		Ok(matrix)
	}
	fn vector(&mut self) -> Result<Vec<C>> {
		let count = self.integer()?;
		self.check_elements(count)?;
		let mut values = Vec::new();
		values
			.try_reserve_exact(count)
			.map_err(|_| Error::Budget("wire vector allocation"))?;
		for _ in 0..count {
			values.push(self.complex()?);
		}
		Ok(values)
	}
	fn text(&mut self) -> Result<&'a str> {
		let count = self.integer()?;
		std::str::from_utf8(self.take(count)?).map_err(|_| Error::Input("wire text encoding"))
	}
}
pub fn decode(bytes: &[u8]) -> Result<FrozenInput> {
	if bytes.len() > IoPolicy::default().max_bytes {
		return Err(Error::Budget("wire bytes"));
	}
	let mut reader = Reader { bytes, position: 0 };
	if reader.take(8)? != MAGIC {
		return Err(Error::Input("wire version"));
	}
	let workflow = match reader.integer()? {
		0 => Workflow::Embedded,
		1 => Workflow::Overlap,
		_ => return Err(Error::Input("wire workflow")),
	};
	let route = route(reader.integer()?)?;
	let verification_tolerance = reader.real()?;
	let alpha = reader.real()?;
	let original = [reader.integer()?, reader.integer()?];
	let padded = [reader.integer()?, reader.integer()?];
	let u = reader.matrix()?;
	let left = reader.matrix()?;
	let right = reader.matrix()?;
	let block = StoredBlockEncoding::builder(u, left, right)
		.metadata(alpha, original, padded)
		.build(IoPolicy::default())?;
	let input = reader.vector()?;
	let reference = reader.vector()?;
	let qsp = quest_qsvt_io::read_qsp_execution_json_with_tolerance(
		reader.text()?,
		IoPolicy::default(),
		verification_tolerance,
	)?;
	if matches!(qsp, quest_qsvt_io::QspInput::Polynomial(_)) {
		return Err(Error::Input("distributed payload must already be frozen"));
	}
	let evidence = serde_json::from_str(reader.text()?)?;
	if reader.position != bytes.len() {
		return Err(Error::Input("trailing application wire data"));
	}
	Ok(FrozenInput {
		workflow,
		route,
		verification_tolerance,
		block,
		qsp,
		input,
		reference,
		evidence,
	})
}

#[cfg(test)]
mod tests {
	use super::*;
	use googletest::prelude::*;
	use quest_qsvt_io::QspInput;
	#[gtest]
	fn frozen_wire_roundtrips_exact_words_and_rejects_truncation() -> googletest::Result<()> {
		let matrix = Mat::from_fn(2, 2, |r, c| {
			if r == c {
				C::new(0.3, 0.4)
			} else {
				C::new(-0.0, f64::from_bits(1))
			}
		});
		let basis = Mat::from_fn(2, 1, |r, _| C::new(if r == 0 { 1.0 } else { 0.0 }, 0.0));
		let block = StoredBlockEncoding::builder(matrix, basis.clone(), basis)
			.metadata(2.0, [1, 1], [1, 1])
			.build(IoPolicy::default())?;
		let qsp = QspInput::GeneralizedMatrices(
			quest_qsp::ControlSequence::builder()
				.angles(&[0.1, -0.2], &[0.3, 0.4])?
				.build()?,
		);
		let input = FrozenInput {
			verification_tolerance: 1e-11,
			workflow: Workflow::Overlap,
			route: TransformRoute::MultiplicationOdd,
			block,
			qsp,
			input: vec![C::new(1.0, 0.0)],
			reference: vec![C::new(0.0, 1.0)],
			evidence: serde_json::json!({"certified":false}),
		};
		let encoded = encode(&input)?;
		let decoded = decode(&encoded)?;
		expect_eq!(encode(&decoded)?, encoded);
		let mut truncated = encoded;
		truncated.pop();
		expect_true!(decode(&truncated).is_err());
		Ok(())
	}

	#[gtest]
	fn imported_angle_wire_preserves_source_and_admitted_matrix_words() -> googletest::Result<()> {
		let matrix = Mat::from_fn(2, 2, |r, c| C::new(if r == c { 1.0 } else { 0.0 }, 0.0));
		let basis = Mat::from_fn(2, 1, |r, _| C::new(if r == 0 { 1.0 } else { 0.0 }, 0.0));
		let block = StoredBlockEncoding::builder(matrix, basis.clone(), basis)
			.metadata(1.0, [1, 1], [1, 1])
			.build(IoPolicy::default())?;
		let qsp = quest_qsvt_io::read_qsp_json(
			r#"{"psi":[0.2,-0.4],"phi":[-0.7,1.2]}"#,
			IoPolicy::default(),
		)?;
		let input = FrozenInput {
			verification_tolerance: 1e-11,
			workflow: Workflow::Embedded,
			route: TransformRoute::Direct,
			block,
			qsp,
			input: vec![C::new(1.0, 0.0)],
			reference: vec![],
			evidence: serde_json::json!({}),
		};
		let encoded = encode(&input)?;
		let decoded = decode(&encoded)?;
		let QspInput::GeneralizedAngles(original) = &input.qsp else {
			return fail!("source lost");
		};
		let QspInput::GeneralizedAngles(received) = &decoded.qsp else {
			return fail!("source lost on worker");
		};
		expect_eq!(received.psi(), original.psi());
		expect_eq!(received.phi(), original.phi());
		let words = |controls: &quest_qsp::ControlSequence| {
			controls
				.matrices()
				.iter()
				.flat_map(|matrix| {
					matrix
						.iter()
						.flat_map(|row| row.iter().flat_map(|z| [z.re.to_bits(), z.im.to_bits()]))
				})
				.collect::<Vec<_>>()
		};
		expect_eq!(words(received.controls()), words(original.controls()));
		Ok(())
	}
}
