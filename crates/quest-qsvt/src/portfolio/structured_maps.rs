//! Label-preserving SCC arithmetic maps for periodic and nonwrapping tensor bands.
//!
//! Column labels are (d,m) with m the input coordinate; the column map is
//! identity and the row map adds one constant tensor offset controlled on d.
//! Data rotations commute with these maps because d is preserved. The PREP
//! variant therefore uses the p=1/2 identity, with explicit out-of-range flags.
#![allow(
	clippy::arithmetic_side_effects,
	clippy::indexing_slicing,
	reason = "Complete widths, input capacity, term ranges and replay work are admitted"
)]
use super::{
	PortfolioLimits, PortfolioResources, add, admit, bits, count_with_work, hash, mul, targets,
	word,
};
use crate::{
	CompactProjector, Complex64, EncodingDescriptor, EncodingErrors, EncodingLayout, Error,
	NumericalPolicy, ReplayEncoding, ReplayGate, ReplayKind, Result, ShiftRegister,
	TensorShiftEncoding, state_preparation::AmplitudePreparation,
};
use std::sync::Arc;
/// Per-axis boundary convention. Zero boundary discards every wrapped entry.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Boundary {
	Periodic,
	Zero,
}
/// Uniform data rotation base, or p=1/2 PREP with label-preserving maps.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StructuredScheme {
	Base,
	Prep,
}
/// One repeated coefficient and signed tensor offset in first-axis-fast order.
#[derive(Clone, Debug)]
pub struct ArithmeticStencilTerm {
	pub weight: Complex64,
	pub offsets: Vec<i64>,
}
#[derive(Clone, Debug)]
struct Frozen {
	weight: Complex64,
	offsets: Vec<i64>,
	shift: TensorShiftEncoding,
	theta: f64,
	phase: f64,
}
#[derive(Debug)]
struct Data {
	widths: Vec<usize>,
	terms: Vec<Frozen>,
	prepare: Option<AmplitudePreparation>,
	scheme: StructuredScheme,
	boundary: Boundary,
	system: usize,
	colors: usize,
	color_width: usize,
	descriptor: EncodingDescriptor,
	resources: PortfolioResources,
}
/// Explicit arithmetic circulant, bounded-band Toeplitz or tensor stencil encoding.
///
/// Only power-of-two axes and distinct offset tuples are admitted. No matrix,
/// permutation or global gate table is constructed.
#[derive(Clone, Debug)]
pub struct ArithmeticStencil(Arc<Data>);
impl ArithmeticStencil {
	/// # Errors
	/// Rejects invalid tensor bands, duplicate maps, nonfinite/zero coefficients and limits.
	#[allow(
		clippy::too_many_lines,
		reason = "Construction admits axes, arithmetic recipes and PREP through one ownership transition"
	)]
	pub fn new(
		widths: Vec<usize>,
		mut terms: Vec<ArithmeticStencilTerm>,
		boundary: Boundary,
		scheme: StructuredScheme,
		l: PortfolioLimits,
	) -> Result<Self> {
		if widths.is_empty() || terms.is_empty() || widths.contains(&0) {
			return Err(Error::Encoding("empty arithmetic stencil/axis"));
		}
		let system = widths.iter().try_fold(0, |n, w| add(n, *w))?;
		let dimension = bits(system)?;
		let input = add(
			mul(widths.capacity(), size_of::<usize>())?,
			mul(terms.capacity(), size_of::<ArithmeticStencilTerm>())?,
		)?;
		let mut peak = add(input, 8192)?;
		let tuple_work = mul(mul(terms.len(), terms.len())?, add(widths.len(), 8)?)?;
		// Input records, plus each eventual shift descriptor's per-axis records.
		// Charge even zero-weight input provenance before any hash/term loop.
		let identity_work = mul(
			terms.len(),
			add(
				crate::record_fingerprint_work(add(3, mul(2, widths.len())?)?)?,
				mul(widths.len(), crate::record_fingerprint_work(3)?)?,
			)?,
		)?;
		let compile_floor = add(tuple_work, identity_work)?;
		if peak > l.max_bytes
			|| compile_floor > l.max_compile_work
			|| terms.len() > l.max_table_entries
		{
			return Err(Error::Budget("arithmetic input/tuple validation"));
		}
		// Every input offset allocation is already live while the first record is
		// validated/hashed. Admit all capacities before entering that term loop.
		peak = terms.iter().try_fold(peak, |bytes, term| {
			add(bytes, mul(term.offsets.capacity(), size_of::<i64>())?)
		})?;
		if peak > l.max_bytes {
			return Err(Error::Budget("arithmetic input offsets"));
		}
		let mut beta = 0.0_f64;
		let mut nominal = 0.0;
		let mut identity = 0u64;
		for (i, term) in terms.iter().enumerate() {
			if term.offsets.len() != widths.len()
				|| !term.weight.re.is_finite()
				|| !term.weight.im.is_finite()
			{
				return Err(Error::Encoding("invalid arithmetic coefficient/axis count"));
			}
			if terms[..i].iter().any(|t| t.offsets == term.offsets) {
				return Err(Error::Encoding("duplicate arithmetic offset tuple"));
			}
			let magnitude = term.weight.norm();
			if !magnitude.is_finite() {
				return Err(Error::NonFinite);
			}
			beta = beta.max(magnitude);
			nominal += magnitude;

			for (&w, &offset) in widths.iter().zip(&term.offsets) {
				let dim =
					i64::try_from(bits(w)?).map_err(|_| Error::Budget("signed arithmetic axis"))?;
				if offset <= -dim || offset >= dim {
					return Err(Error::Encoding("band offset exceeds tensor axis"));
				}
				// Validate conversion before the infallible streaming hash iterator.
				word(w)?;
			}
			let term_hash = crate::record_fingerprint(
				0x4152_4952_4543_5632,
				[
					term.weight.re.to_bits(),
					term.weight.im.to_bits(),
					word(widths.len())?,
				]
				.into_iter()
				.chain(widths.iter().zip(&term.offsets).flat_map(|(&w, &offset)| {
					// The conversion was checked above; this fallback is unreachable.
					[
						u64::try_from(w).unwrap_or(u64::MAX),
						u64::from_le_bytes(offset.to_le_bytes()),
					]
				})),
			);
			identity = identity.wrapping_add(term_hash);
		}
		terms.retain(|t| t.weight.re != 0.0 || t.weight.im != 0.0);
		if terms.is_empty() || !nominal.is_finite() {
			return Err(Error::Encoding("zero/unrepresentable arithmetic stencil"));
		}
		let colors = terms
			.len()
			.checked_next_power_of_two()
			.ok_or(Error::Budget("arithmetic labels"))?;
		let color_width =
			usize::try_from(colors.ilog2()).map_err(|_| Error::Budget("arithmetic labels"))?;
		bits(add(add(system, 2)?, color_width)?)?;
		peak = add(peak, mul(terms.len(), size_of::<Frozen>() + 64)?)?;
		if peak > l.max_bytes
			|| terms.len() > l.max_table_entries
			|| mul(mul(terms.len(), terms.len())?, add(widths.len(), 8)?)? > l.max_compile_work
		{
			return Err(Error::Budget("arithmetic construction"));
		}
		let mut frozen = crate::matching::reserve(terms.len())?;
		for term in terms {
			let mut shifts = crate::matching::reserve(widths.len())?;
			let mut start = 0;
			for (&w, &offset) in widths.iter().zip(&term.offsets) {
				let dim = i64::try_from(bits(w)?).map_err(|_| Error::Budget("arithmetic axis"))?;
				shifts.push(ShiftRegister::new(
					start,
					w,
					usize::try_from(offset.rem_euclid(dim))
						.map_err(|_| Error::Budget("arithmetic offset"))?,
				)?);
				start = add(start, w)?;
			}
			let shift = TensorShiftEncoding::new(
				system,
				shifts,
				NumericalPolicy {
					max_bytes: l.max_bytes.saturating_sub(peak),
				},
			)?;
			peak = add(peak, shift.retained_bytes()?)?;
			if peak > l.max_bytes {
				return Err(Error::Budget("arithmetic shifts"));
			}
			frozen.push(Frozen {
				theta: 2.0 * (term.weight.norm() / beta).clamp(0.0, 1.0).acos(),
				phase: term.weight.arg(),
				weight: term.weight,
				offsets: term.offsets,
				shift,
			});
		}
		let prepare = if scheme == StructuredScheme::Prep {
			let mut amplitudes = crate::matching::reserve(frozen.len())?;
			for t in &frozen {
				amplitudes.push(Complex64::new(t.weight.norm().sqrt(), 0.0));
			}
			let mut pl = l.preparation;
			pl.max_bytes = pl.max_bytes.min(l.max_bytes.saturating_sub(peak));
			pl.max_compile_work = pl.max_compile_work.min(
				l.max_compile_work
					.checked_sub(compile_floor)
					.ok_or(Error::Budget("arithmetic identity work"))?,
			);
			pl.max_gates = pl.max_gates.min(l.max_gates);
			Some(AmplitudePreparation::new(&amplitudes, pl)?)
		} else {
			None
		};
		let normalization = if let Some(p) = &prepare {
			p.norm() * p.norm()
		} else {
			beta * f64::from(
				u32::try_from(colors).map_err(|_| Error::Budget("arithmetic normalization"))?,
			)
		};
		let source = hash([
			0x5343_4353_4f55_5232,
			word(system)?,
			u64::from(boundary == Boundary::Zero),
			identity,
		]);
		let construction = frozen.iter().try_fold(
			hash([source, u64::from(scheme == StructuredScheme::Prep)]),
			|h, t| -> Result<u64> {
				Ok(hash([
					h,
					t.shift.descriptor()?.construction_identity,
					t.weight.re.to_bits(),
					t.weight.im.to_bits(),
				]))
			},
		)?;
		let descriptor = descriptor(
			system,
			2,
			color_width,
			dimension,
			dimension,
			normalization,
			source,
			construction,
		)?;
		let mut retained = size_of::<Data>()
			+ 64
			+ mul(widths.capacity(), size_of::<usize>())?
			+ mul(frozen.capacity(), size_of::<Frozen>())?;
		for t in &frozen {
			retained = add(
				retained,
				add(
					mul(t.offsets.capacity(), size_of::<i64>())?,
					t.shift.retained_bytes()?,
				)?,
			)?;
		}
		let (pr_gates, pr_work) = if let Some(p) = &prepare {
			retained = add(retained, p.resources().retained_bytes)?;
			peak = add(peak, p.resources().construction_peak_bytes)?;
			(
				mul(p.resources().elementary_gates, 2)?,
				p.resources().compile_work,
			)
		} else {
			(mul(color_width, 2)?, 0)
		};
		let resources = PortfolioResources {
			table_entries: frozen.len(),
			workspace_qubits: add(2, color_width)?,
			preparation_gates: pr_gates,
			preparation_compile_work: pr_work,
			compile_work: add(pr_work, compile_floor)?,
			retained_bytes: retained,
			construction_peak_bytes: peak.max(add(retained, 8192)?),
			normalization_roundoff: if scheme == StructuredScheme::Prep {
				(normalization - nominal).abs()
			} else {
				0.0
			},
			..PortfolioResources::default()
		};
		admit(resources, l)?;
		let mut result = Self(Arc::new(Data {
			widths,
			terms: frozen,
			prepare,
			scheme,
			boundary,
			system,
			colors,
			color_width,
			descriptor,
			resources,
		}));
		let mut compile_work = result.0.resources.compile_work;
		let gates = count_with_work(&result, l, &mut compile_work)?;
		let data =
			Arc::get_mut(&mut result.0).ok_or(Error::Encoding("arithmetic recipe ownership"))?;
		data.resources.elementary_gates = gates;
		data.resources.compile_work = compile_work;
		admit(data.resources, l)?;
		Ok(result)
	}
	#[must_use]
	pub fn resources(&self) -> PortfolioResources {
		self.0.resources
	}
	/// Reversible (d,m) -> (d,row) modular arithmetic; adjoint recovers m.
	/// # Errors
	/// Rejects out-of-domain labels/coordinates.
	pub fn row_map(&self, label: usize, coordinate: usize, adjoint: bool) -> Result<usize> {
		let term = self
			.0
			.terms
			.get(label)
			.ok_or(Error::Encoding("arithmetic label"))?;
		term.shift.map_index(coordinate, adjoint)
	}
	/// Source-coordinate validity prior to the modular row map.
	/// # Errors
	/// Rejects out-of-domain labels/coordinates.
	pub fn valid(&self, label: usize, coordinate: usize) -> Result<bool> {
		if coordinate >= bits(self.0.system)? {
			return Err(Error::Encoding("arithmetic coordinate"));
		}
		let term = self
			.0
			.terms
			.get(label)
			.ok_or(Error::Encoding("arithmetic label"))?;
		if self.0.boundary == Boundary::Periodic {
			return Ok(true);
		}
		let mut start = 0;
		for (&width, &offset) in self.0.widths.iter().zip(&term.offsets) {
			let dim = bits(width)?;
			let x = i64::try_from((coordinate >> start) & (dim - 1))
				.map_err(|_| Error::Budget("arithmetic coordinate"))?;
			let dim = i64::try_from(dim).map_err(|_| Error::Budget("arithmetic coordinate"))?;
			if x + offset < 0 || x + offset >= dim {
				return Ok(false);
			}
			start += width;
		}
		Ok(true)
	}
	fn prepare(&self, adjoint: bool, v: &mut dyn FnMut(ReplayGate) -> Result<()>) -> Result<()> {
		let start = add(self.0.system, 2)?;
		let map = targets(start, self.0.color_width)?;
		if let Some(p) = &self.0.prepare {
			p.visit_mapped_gates(&map[..self.0.color_width], 0, 0, adjoint, v)
		} else {
			for i in 0..self.0.color_width {
				let bit = if adjoint {
					self.0.color_width - 1 - i
				} else {
					i
				};
				v(ReplayGate {
					kind: ReplayKind::H,
					target: Some(start + bit),
					control_mask: 0,
					control_value: 0,
				})?;
			}
			Ok(())
		}
	}
	fn range_flag(
		&self,
		label: usize,
		term: &Frozen,
		adjoint: bool,
		v: &mut dyn FnMut(ReplayGate) -> Result<()>,
	) -> Result<()> {
		if self.0.boundary == Boundary::Periodic {
			return Ok(());
		}
		let mut ranges = [(0usize, 0usize, 0usize, 0usize); 64];
		let mut start = 2;
		for (i, (&w, &offset)) in self.0.widths.iter().zip(&term.offsets).enumerate() {
			let dim = bits(w)?;
			let delta = usize::try_from(offset.unsigned_abs())
				.map_err(|_| Error::Budget("arithmetic delta"))?;
			let valid = if offset >= 0 {
				(0, dim - delta)
			} else {
				(delta, dim)
			};
			ranges[i] = (start, w, valid.0, valid.1);
			start += w;
		}
		let mask = (bits(self.0.color_width)? - 1) << (self.0.system + 2);
		let value = label << (self.0.system + 2);
		for ordinal in 0..self.0.widths.len() {
			let axis = if adjoint {
				self.0.widths.len() - 1 - ordinal
			} else {
				ordinal
			};
			let (start, w, low, high) = ranges[axis];
			let dim = bits(w)?;
			let invalid = if low > 0 { (0, low) } else { (high, dim) };
			if invalid.0 == invalid.1 {
				continue;
			}
			let mut selected = ranges;
			selected[axis] = (start, w, invalid.0, invalid.1);
			visit_rectangles(
				&selected[..=axis],
				0,
				mask,
				value,
				adjoint,
				&mut |mask, value| {
					v(ReplayGate {
						kind: ReplayKind::X,
						target: Some(1),
						control_mask: mask,
						control_value: value,
					})
				},
			)?;
		}
		Ok(())
	}
	fn label(
		&self,
		label: usize,
		adjoint: bool,
		v: &mut dyn FnMut(ReplayGate) -> Result<()>,
	) -> Result<()> {
		let term = self.0.terms.get(label);
		let mask = (bits(self.0.color_width)? - 1) << (self.0.system + 2);
		let value = label << (self.0.system + 2);
		let rotate = |v: &mut dyn FnMut(ReplayGate) -> Result<()>| -> Result<()> {
			if self.0.scheme == StructuredScheme::Base {
				v(ReplayGate {
					kind: ReplayKind::Ry(if adjoint {
						-term.map_or(std::f64::consts::PI, |t| t.theta)
					} else {
						term.map_or(std::f64::consts::PI, |t| t.theta)
					}),
					target: Some(0),
					control_mask: mask,
					control_value: value,
				})?;
			}
			Ok(())
		};
		let phase = ReplayGate {
			kind: ReplayKind::Phase(term.map_or(0.0, |t| if adjoint { -t.phase } else { t.phase })),
			target: None,
			control_mask: if self.0.scheme == StructuredScheme::Base {
				mask | 1
			} else {
				mask
			},
			control_value: value,
		};
		let permute = |v: &mut dyn FnMut(ReplayGate) -> Result<()>| -> Result<()> {
			if let Some(t) = term {
				let map = targets(2, self.0.system)?;
				t.shift
					.visit_mapped_replay(&map[..self.0.system], mask, value, adjoint, v)?;
			}
			Ok(())
		};
		if adjoint {
			permute(v)?;
			v(phase)?;
			rotate(v)?;
			if let Some(t) = term {
				self.range_flag(label, t, true, v)?;
			}
		} else {
			if let Some(t) = term {
				self.range_flag(label, t, false, v)?;
			}
			rotate(v)?;
			v(phase)?;
			permute(v)?;
		}
		Ok(())
	}
}
// Disjoint bit cubes implement a Cartesian interval without an expanded index set.
fn visit_rectangles(
	ranges: &[(usize, usize, usize, usize)],
	axis: usize,
	mask: usize,
	value: usize,
	reverse: bool,
	v: &mut dyn FnMut(usize, usize) -> Result<()>,
) -> Result<()> {
	if axis == ranges.len() {
		return v(mask, value);
	}
	let (start, width, low, high) = ranges[axis];
	if low == high {
		return Ok(());
	}
	let mut boundary = if reverse { high } else { low };
	while if reverse {
		boundary > low
	} else {
		boundary < high
	} {
		let exponent = if reverse {
			boundary.trailing_zeros().min((boundary - low).ilog2())
		} else {
			boundary.trailing_zeros().min((high - boundary).ilog2())
		};
		let size =
			bits(usize::try_from(exponent).map_err(|_| Error::Budget("arithmetic range cube"))?)?;
		let base = if reverse { boundary - size } else { boundary };
		let fixed = (bits(width)? - 1) & !(size - 1);
		visit_rectangles(
			ranges,
			axis + 1,
			mask | (fixed << start),
			value | (base << start),
			reverse,
			v,
		)?;
		if reverse {
			boundary -= size;
		} else {
			boundary += size;
		}
	}
	Ok(())
}
impl ReplayEncoding for ArithmeticStencil {
	fn descriptor(&self) -> Result<EncodingDescriptor> {
		Ok(self.0.descriptor.clone())
	}
	fn retained_bytes(&self) -> Result<usize> {
		Ok(self.0.resources.retained_bytes)
	}
	fn visit_replay(
		&self,
		adjoint: bool,
		v: &mut dyn FnMut(ReplayGate) -> Result<()>,
	) -> Result<()> {
		self.prepare(false, v)?;
		for ordinal in 0..self.0.colors {
			let label = if adjoint {
				self.0.colors - 1 - ordinal
			} else {
				ordinal
			};
			self.label(label, adjoint, v)?;
		}
		self.prepare(true, v)
	}
}
#[allow(
	clippy::too_many_arguments,
	reason = "Scalar layout, logical dimensions, normalization and both identities define one descriptor"
)]
pub(super) fn descriptor(
	system: usize,
	flags: usize,
	labels: usize,
	rows: usize,
	cols: usize,
	normalization: f64,
	source: u64,
	construction: u64,
) -> Result<EncodingDescriptor> {
	let width = add(add(system, flags)?, labels)?;
	let system_mask = (bits(system)? - 1) << flags;
	let workspace_mask = (bits(width)? - 1) & !system_mask;
	let d = EncodingDescriptor {
		rows,
		cols,
		normalization,
		layout: EncodingLayout {
			num_qubits: width,
			system_mask,
			workspace_mask,
			clean_workspace_mask: workspace_mask,
			clean_workspace_value: 0,
		},
		left: CompactProjector {
			fixed_mask: workspace_mask,
			fixed_value: 0,
			logical_range: 0..rows,
		},
		right: CompactProjector {
			fixed_mask: workspace_mask,
			fixed_value: 0,
			logical_range: 0..cols,
		},
		errors: EncodingErrors {
			preparation: None,
			encoding: None,
			binary64_parameters: true,
		},
		source_identity: source,
		construction_identity: construction,
	};
	d.validate()?;
	Ok(d)
}
