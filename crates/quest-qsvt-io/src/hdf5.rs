//! Serial, read-only HDF5 interchange matching the C++ scientific file layouts.
//!
//! Distributed applications call these readers on their coordinating root. The
//! stored block encoding contains shape-checked data, not a unitarity certificate.
use crate::{Complex64, Error, IoPolicy, Result, SparseFormat, SparseMatrix, finite};
use faer::{Mat, MatRef};
use hdf5_metno::{
    Container, Dataset, File, H5Type, Location,
    types::{
        FixedAscii, FixedUnicode, FloatSize, IntSize, TypeDescriptor, VarLenAscii, VarLenUnicode,
    },
};
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Default, H5Type)]
#[repr(C)]
struct H5ppComplex64 {
    real: f64,
    imag: f64,
}
#[derive(Clone, Copy, Default, H5Type)]
#[repr(C)]
struct H5ppComplex32 {
    real: f32,
    imag: f32,
}

/// Read-only file owner. Every read retains owned project data after file Drop.
pub struct Hdf5Input {
    file: File,
    policy: IoPolicy,
}
/// File selection and bounded scientific payload policy before opening.
pub struct Hdf5InputBuilder {
    path: PathBuf,
    policy: IoPolicy,
}
impl Hdf5InputBuilder {
    #[must_use]
    pub const fn policy(mut self, policy: IoPolicy) -> Self {
        self.policy = policy;
        self
    }
    /// # Errors
    /// Reports file opening errors; no file is created or modified.
    pub fn open(self) -> Result<Hdf5Input> {
        Hdf5Input::open(self.path, self.policy)
    }
}
impl Hdf5Input {
    #[must_use]
    pub fn builder(path: impl AsRef<Path>) -> Hdf5InputBuilder {
        Hdf5InputBuilder {
            path: path.as_ref().into(),
            policy: IoPolicy::default(),
        }
    }
    /// # Errors
    /// Reports file opening errors; no file is created or modified.
    pub fn open(path: impl AsRef<Path>, policy: IoPolicy) -> Result<Self> {
        Ok(Self {
            file: File::open(path)?,
            policy,
        })
    }
    /// Read `/matrix/dense` or compressed `/matrix/{data,indices,indptr}`.
    /// Sparse duplicate entries remain ordered until explicit densification.
    /// # Errors
    /// Rejects ambiguous formats, unsupported dtype, invalid shapes/indices, nonfinite data and budget failures.
    pub fn read_matrix(&self) -> Result<StoredMatrix> {
        let group = self.file.group("/matrix")?;
        let shape_attribute = group.attr("shape")?;
        let shape = integer_values(&shape_attribute, self.policy)?;
        let [rows, cols] = shape.as_slice() else {
            return Err(Error::Format("matrix shape must contain two dimensions"));
        };
        let (rows, cols) = (*rows, *cols);
        dimensions(rows, cols, self.policy)?;
        let format = optional_text(&group, "format")?;
        let dtype = optional_text(&group, "dtype")?;
        let dense = group.link_exists("dense");
        let sparse = ["data", "indices", "indptr"]
            .iter()
            .any(|name| group.link_exists(name));
        if dense == sparse {
            return Err(Error::Format("ambiguous or absent matrix storage"));
        }
        let storage = if dense {
            if format.as_deref().is_some_and(|value| value != "dense") {
                return Err(Error::Format("matrix format disagrees with dense storage"));
            }
            let data = group.dataset("dense")?;
            if data.shape() != [rows, cols] {
                return Err(Error::Format("dense matrix shape mismatch"));
            }
            MatrixStorage::Dense(read_dense(&data, dtype.as_deref(), self.policy)?)
        } else {
            let data = group.dataset("data")?;
            let indices = group.dataset("indices")?;
            let pointers = group.dataset("indptr")?;
            let entries = vector_length(&data)?;
            if vector_length(&indices)? != entries {
                return Err(Error::Format("sparse index/data length mismatch"));
            }
            let pointer_count = vector_length(&pointers)?;
            let row_pointers = rows.checked_add(1).ok_or(Error::Budget("sparse rows"))?;
            let col_pointers = cols.checked_add(1).ok_or(Error::Budget("sparse columns"))?;
            let format = match format.as_deref() {
                Some("csr") => SparseFormat::Csr,
                Some("csc") => SparseFormat::Csc,
                Some(_) => return Err(Error::Format("unsupported sparse format")),
                None if pointer_count == row_pointers && pointer_count != col_pointers => {
                    SparseFormat::Csr
                }
                None if pointer_count == col_pointers && pointer_count != row_pointers => {
                    SparseFormat::Csc
                }
                None => {
                    return Err(Error::Format(
                        "ambiguous sparse format requires csr/csc attribute",
                    ));
                }
            };
            let expected = match format {
                SparseFormat::Csr => row_pointers,
                SparseFormat::Csc => col_pointers,
            };
            if pointer_count != expected {
                return Err(Error::Format("sparse pointer length mismatch"));
            }
            self.policy.check_sparse_storage(entries, pointer_count)?;
            let base = if group.attr_names()?.iter().any(|name| name == "index_base") {
                scalar_integer(&group, "index_base", self.policy)?
            } else {
                0
            };
            if base > 1 {
                return Err(Error::Format("sparse index base must be zero or one"));
            }
            let values = complex_values(&data, dtype.as_deref(), self.policy)?;
            let indices = integer_values(&indices, self.policy)?;
            let pointers = integer_values(&pointers, self.policy)?;
            let builder = SparseMatrix::builder(rows, cols).format(format);
            let builder = if base == 1 {
                builder.one_based()
            } else {
                builder
            };
            MatrixStorage::Sparse(
                builder
                    .entries(values, indices, pointers)
                    .build(self.policy)?,
            )
        };
        Ok(StoredMatrix {
            rows,
            cols,
            storage,
        })
    }
    /// Read `/state/vector`, requiring matching `length` and `dtype` attributes.
    /// # Errors
    /// Rejects unsupported types, inconsistent length, nonfinite values and budgets.
    pub fn read_state_vector(&self) -> Result<Vec<Complex64>> {
        let dataset = self.file.dataset("/state/vector")?;
        let length = vector_length(&dataset)?;
        if length == 0 || length > self.policy.max_dimension {
            return Err(Error::Format("state vector length"));
        }
        if scalar_integer(&dataset, "length", self.policy)? != length {
            return Err(Error::Format("state vector length attribute mismatch"));
        }
        let dtype = text_attribute(&dataset, "dtype")?;
        let values = complex_values(&dataset, Some(&dtype), self.policy)?;
        Ok(values)
    }
    /// Read `U`, `PiL`, `PiR` and positive scale/dimension metadata.
    /// Isometry and unitary numerical admission belongs to the computation layer.
    /// # Errors
    /// Rejects missing/inconsistent metadata, unsupported types, nonfinite values and aggregate budgets.
    pub fn read_block_encoding(&self) -> Result<StoredBlockEncoding> {
        let group = self.file.group("/block_encoding")?;
        let original = [
            scalar_integer(&group, "original_rows", self.policy)?,
            scalar_integer(&group, "original_cols", self.policy)?,
        ];
        let padded = [
            scalar_integer(&group, "padded_rows", self.policy)?,
            scalar_integer(&group, "padded_cols", self.policy)?,
        ];
        let alpha_attribute = group.attr("alpha")?;
        if alpha_attribute.size() != 1
            || !matches!(
                alpha_attribute.dtype()?.to_descriptor()?,
                TypeDescriptor::Float(FloatSize::U8)
            )
        {
            return Err(Error::Format("block alpha must be scalar float64"));
        }
        let alpha = alpha_attribute.read_scalar::<f64>()?;
        if !alpha.is_finite() || alpha <= 0. {
            return Err(Error::Format("block alpha must be finite and positive"));
        }
        let [original_rows, original_cols] = original;
        let [padded_rows, padded_cols] = padded;
        dimensions(original_rows, original_cols, self.policy)?;
        dimensions(padded_rows, padded_cols, self.policy)?;
        if original_rows > padded_rows || original_cols > padded_cols {
            return Err(Error::Format("original dimensions exceed padding"));
        }
        let u = group.dataset("U")?;
        let left = group.dataset("PiL")?;
        let right = group.dataset("PiR")?;
        let [dimension, columns] = matrix_shape(&u)?;
        let canonical = padded_rows
            .checked_add(padded_cols)
            .ok_or(Error::Budget("block dimension"))?;
        if dimension != columns
            || !dimension.is_power_of_two()
            || dimension < canonical
            || matrix_shape(&left)? != [dimension, padded_rows]
            || matrix_shape(&right)? != [dimension, padded_cols]
        {
            return Err(Error::Format("block encoding dataset shapes disagree"));
        }
        let count =
            [columns, padded_rows, padded_cols]
                .into_iter()
                .try_fold(0usize, |total, width| {
                    dense_storage_count(dimension, width, self.policy)?
                        .checked_add(total)
                        .ok_or(Error::Budget("block aggregate storage"))
                })?;
        self.policy.check(count, 3)?;
        Ok(StoredBlockEncoding {
            u: read_dense(&u, None, self.policy)?,
            pi_left: read_dense(&left, None, self.policy)?,
            pi_right: read_dense(&right, None, self.policy)?,
            alpha,
            original,
            padded,
        })
    }
}

enum MatrixStorage {
    Dense(Mat<Complex64>),
    Sparse(SparseMatrix),
}
/// Owned dense or compressed matrix preserving rectangular shape and logical order.
pub struct StoredMatrix {
    rows: usize,
    cols: usize,
    storage: MatrixStorage,
}
impl StoredMatrix {
    #[must_use]
    pub const fn dimensions(&self) -> [usize; 2] {
        [self.rows, self.cols]
    }
    #[must_use]
    pub fn dense(&self) -> Option<MatRef<'_, Complex64>> {
        match &self.storage {
            MatrixStorage::Dense(matrix) => Some(matrix.as_ref()),
            MatrixStorage::Sparse(_) => None,
        }
    }
    #[must_use]
    pub const fn sparse(&self) -> Option<&SparseMatrix> {
        match &self.storage {
            MatrixStorage::Sparse(matrix) => Some(matrix),
            MatrixStorage::Dense(_) => None,
        }
    }
    /// Consume dense storage directly or explicitly densify compressed entries.
    /// # Errors
    /// Rejects shape/budget violations or nonfinite duplicate sums.
    pub fn into_dense(self, policy: IoPolicy) -> Result<Mat<Complex64>> {
        let count = dense_storage_count(self.rows, self.cols, policy)?;
        policy.check(count, 1)?;
        match self.storage {
            MatrixStorage::Dense(matrix) => Ok(matrix),
            MatrixStorage::Sparse(matrix) => matrix.densify(policy),
        }
    }
}
/// Shape-checked stored data; file metadata makes no mathematical certificate.
pub struct StoredBlockEncoding {
    u: Mat<Complex64>,
    pi_left: Mat<Complex64>,
    pi_right: Mat<Complex64>,
    alpha: f64,
    original: [usize; 2],
    padded: [usize; 2],
}
impl StoredBlockEncoding {
    #[must_use]
    pub fn u(&self) -> MatRef<'_, Complex64> {
        self.u.as_ref()
    }
    #[must_use]
    pub fn pi_left(&self) -> MatRef<'_, Complex64> {
        self.pi_left.as_ref()
    }
    #[must_use]
    pub fn pi_right(&self) -> MatRef<'_, Complex64> {
        self.pi_right.as_ref()
    }
    #[must_use]
    pub const fn alpha(&self) -> f64 {
        self.alpha
    }
    #[must_use]
    pub const fn original_dimensions(&self) -> [usize; 2] {
        self.original
    }
    #[must_use]
    pub const fn padded_dimensions(&self) -> [usize; 2] {
        self.padded
    }
}
/// # Errors
/// Propagates file, format, finite-value and budget failures.
pub fn read_matrix(path: impl AsRef<Path>, policy: IoPolicy) -> Result<StoredMatrix> {
    Hdf5Input::open(path, policy)?.read_matrix()
}
/// # Errors
/// Propagates file, format, finite-value and budget failures.
pub fn read_state_vector(path: impl AsRef<Path>, policy: IoPolicy) -> Result<Vec<Complex64>> {
    Hdf5Input::open(path, policy)?.read_state_vector()
}
/// # Errors
/// Propagates file, format, finite-value and budget failures.
pub fn read_block_encoding(
    path: impl AsRef<Path>,
    policy: IoPolicy,
) -> Result<StoredBlockEncoding> {
    Hdf5Input::open(path, policy)?.read_block_encoding()
}

fn check_bytes(bytes: usize, policy: IoPolicy) -> Result<()> {
    if bytes > policy.max_bytes || isize::try_from(bytes).is_err() {
        return Err(Error::Budget("HDF5 payload storage"));
    }
    Ok(())
}
const fn dimensions(rows: usize, cols: usize, policy: IoPolicy) -> Result<()> {
    if rows == 0 || cols == 0 || rows > policy.max_dimension || cols > policy.max_dimension {
        return Err(Error::Format("matrix dimensions"));
    }
    Ok(())
}
fn dense_storage_count(rows: usize, cols: usize, policy: IoPolicy) -> Result<usize> {
    dimensions(rows, cols, policy)?;
    rows.checked_next_multiple_of(4)
        .and_then(|rows| rows.checked_mul(cols))
        .ok_or(Error::Budget("dense shape"))
}
fn vector_length(data: &Container) -> Result<usize> {
    let shape = data.shape();
    let [length] = shape.as_slice() else {
        return Err(Error::Format("dataset must be one-dimensional"));
    };
    Ok(*length)
}
fn matrix_shape(data: &Container) -> Result<[usize; 2]> {
    let shape = data.shape();
    let [rows, cols] = shape.as_slice() else {
        return Err(Error::Format("dataset must be two-dimensional"));
    };
    Ok([*rows, *cols])
}
fn read_buffer<T: H5Type + Copy + Default>(data: &Container, policy: IoPolicy) -> Result<Vec<T>> {
    check_bytes(
        data.size()
            .checked_mul(size_of::<T>())
            .ok_or(Error::Budget("HDF5 buffer"))?,
        policy,
    )?;
    let mut values = Vec::new();
    values
        .try_reserve_exact(data.size())
        .map_err(|_| Error::Budget("HDF5 allocation"))?;
    values.resize(data.size(), T::default());
    if data.as_reader().read_into_raw(&mut values)? != values.len() {
        return Err(Error::Format("HDF5 read count"));
    }
    Ok(values)
}
fn integer_values(data: &Container, policy: IoPolicy) -> Result<Vec<usize>> {
    let descriptor = data.dtype()?.to_descriptor()?;
    let signed = matches!(
        descriptor,
        TypeDescriptor::Integer(IntSize::U4 | IntSize::U8)
    );
    if !signed
        && !matches!(
            descriptor,
            TypeDescriptor::Unsigned(IntSize::U4 | IntSize::U8)
        )
    {
        return Err(Error::Format(
            "indices and dimensions require 32/64-bit integers",
        ));
    }
    check_bytes(
        data.size()
            .checked_mul(16)
            .ok_or(Error::Budget("integer conversion"))?,
        policy,
    )?;
    let mut output = Vec::new();
    output
        .try_reserve_exact(data.size())
        .map_err(|_| Error::Budget("index allocation"))?;
    if signed {
        for value in read_buffer::<i64>(data, policy)? {
            output.push(
                usize::try_from(value).map_err(|_| Error::Format("negative or oversized index"))?,
            );
        }
    } else {
        for value in read_buffer::<u64>(data, policy)? {
            output.push(usize::try_from(value).map_err(|_| Error::Format("oversized index"))?);
        }
    }
    Ok(output)
}
fn scalar_integer(location: &Location, name: &str, policy: IoPolicy) -> Result<usize> {
    let attribute = location.attr(name)?;
    if attribute.size() != 1 {
        return Err(Error::Format("dimension attribute must be scalar"));
    }
    integer_values(&attribute, policy)?
        .first()
        .copied()
        .ok_or(Error::Format("missing scalar index"))
}
fn optional_text(location: &Location, name: &str) -> Result<Option<String>> {
    if location
        .attr_names()?
        .iter()
        .any(|candidate| candidate == name)
    {
        Ok(Some(text_attribute(location, name)?))
    } else {
        Ok(None)
    }
}
fn text_attribute(location: &Location, name: &str) -> Result<String> {
    let attribute = location.attr(name)?;
    if attribute.size() != 1 {
        return Err(Error::Format("text attribute must be scalar"));
    }
    match attribute.dtype()?.to_descriptor()? {
        TypeDescriptor::FixedAscii(size) if size <= 64 => {
            bounded_text(attribute.read_scalar::<FixedAscii<64>>()?.as_bytes())
        }
        TypeDescriptor::FixedUnicode(size) if size <= 64 => {
            bounded_text(attribute.read_scalar::<FixedUnicode<64>>()?.as_bytes())
        }
        TypeDescriptor::VarLenAscii => {
            let value = attribute.read_scalar::<VarLenAscii>()?;
            bounded_text(value.as_bytes())
        }
        TypeDescriptor::VarLenUnicode => {
            let value = attribute.read_scalar::<VarLenUnicode>()?;
            bounded_text(value.as_bytes())
        }
        _ => Err(Error::Format("unsupported or oversized string attribute")),
    }
}
#[derive(Clone, Copy, PartialEq, Eq)]
enum ScalarKind {
    Real32,
    Real64,
    Complex32,
    Complex64,
}
fn declared_kind(value: &str) -> Result<ScalarKind> {
    match value {
        "float32" | "real32" | "f4" => Ok(ScalarKind::Real32),
        "float64" | "real64" | "f8" => Ok(ScalarKind::Real64),
        "complex64" | "c8" => Ok(ScalarKind::Complex32),
        "complex128" | "c16" => Ok(ScalarKind::Complex64),
        _ => Err(Error::Format("unsupported dtype attribute")),
    }
}
fn complex_values(
    data: &Container,
    declared: Option<&str>,
    policy: IoPolicy,
) -> Result<Vec<Complex64>> {
    policy.check(data.size(), 2)?;
    let descriptor = data.dtype()?.to_descriptor()?;
    let (kind, h5pp) = match descriptor {
        TypeDescriptor::Float(FloatSize::U4) => (ScalarKind::Real32, false),
        TypeDescriptor::Float(FloatSize::U8) => (ScalarKind::Real64, false),
        TypeDescriptor::Compound(compound) if compound.fields.len() == 2 => {
            let real = compound
                .fields
                .iter()
                .find(|field| field.name == "r" || field.name == "real");
            let imag = compound
                .fields
                .iter()
                .find(|field| field.name == "i" || field.name == "imag");
            let (Some(real), Some(imag)) = (real, imag) else {
                return Err(Error::Format("complex field names"));
            };
            if real.ty != imag.ty || (real.name == "real") != (imag.name == "imag") {
                return Err(Error::Format("complex field types"));
            }
            let kind = match real.ty {
                TypeDescriptor::Float(FloatSize::U4) => ScalarKind::Complex32,
                TypeDescriptor::Float(FloatSize::U8) => ScalarKind::Complex64,
                _ => return Err(Error::Format("complex field precision")),
            };
            (kind, real.name == "real")
        }
        _ => {
            return Err(Error::Format(
                "dataset requires float32/64 or complex64/128",
            ));
        }
    };
    if declared
        .map(declared_kind)
        .transpose()?
        .is_some_and(|declared| declared != kind)
    {
        return Err(Error::Format("dtype attribute disagrees with dataset"));
    }
    let mut output = Vec::new();
    output
        .try_reserve_exact(data.size())
        .map_err(|_| Error::Budget("complex allocation"))?;
    match (kind, h5pp) {
        (ScalarKind::Real32, _) => {
            for value in read_buffer::<f32>(data, policy)? {
                output.push(finite(Complex64::new(f64::from(value), 0.))?);
            }
        }
        (ScalarKind::Real64, _) => {
            for value in read_buffer::<f64>(data, policy)? {
                output.push(finite(Complex64::new(value, 0.))?);
            }
        }
        (ScalarKind::Complex32, false) => {
            for value in read_buffer::<num_complex::Complex32>(data, policy)? {
                output.push(finite(Complex64::new(
                    f64::from(value.re),
                    f64::from(value.im),
                ))?);
            }
        }
        (ScalarKind::Complex32, true) => {
            for value in read_buffer::<H5ppComplex32>(data, policy)? {
                output.push(finite(Complex64::new(
                    f64::from(value.real),
                    f64::from(value.imag),
                ))?);
            }
        }
        (ScalarKind::Complex64, false) => {
            for value in read_buffer::<Complex64>(data, policy)? {
                output.push(finite(value)?);
            }
        }
        (ScalarKind::Complex64, true) => {
            for value in read_buffer::<H5ppComplex64>(data, policy)? {
                output.push(finite(Complex64::new(value.real, value.imag))?);
            }
        }
    }
    Ok(output)
}
fn read_dense(
    dataset: &Dataset,
    declared: Option<&str>,
    policy: IoPolicy,
) -> Result<Mat<Complex64>> {
    let [rows, cols] = matrix_shape(dataset)?;
    policy.check(dense_storage_count(rows, cols, policy)?, 3)?;
    let values = complex_values(dataset, declared, policy)?;
    let mut matrix = Mat::new();
    matrix
        .try_reserve(rows, cols)
        .map_err(|_| Error::Budget("dense matrix allocation"))?;
    matrix.resize_with(rows, cols, |_, _| Complex64::new(0., 0.));
    for (row, values) in values.chunks(cols).enumerate() {
        for (col, &value) in values.iter().enumerate() {
            matrix[(row, col)] = value;
        }
    }
    Ok(matrix)
}

/// Stored matrices awaiting their positive scale and original/padded dimensions.
pub struct NeedsBlockMetadata;
/// Explicit file metadata, without mathematical admission claims.
pub struct BlockMetadata {
    alpha: f64,
    original: [usize; 2],
    padded: [usize; 2],
}
/// Consuming construction for owned, shape-checked block-encoding storage.
pub struct StoredBlockEncodingBuilder<State = NeedsBlockMetadata> {
    u: Mat<Complex64>,
    pi_left: Mat<Complex64>,
    pi_right: Mat<Complex64>,
    state: State,
}
impl StoredBlockEncoding {
    #[must_use]
    pub const fn builder(
        u: Mat<Complex64>,
        pi_left: Mat<Complex64>,
        pi_right: Mat<Complex64>,
    ) -> StoredBlockEncodingBuilder {
        StoredBlockEncodingBuilder {
            u,
            pi_left,
            pi_right,
            state: NeedsBlockMetadata,
        }
    }
}
impl StoredBlockEncodingBuilder {
    #[must_use]
    pub fn metadata(
        self,
        alpha: f64,
        original: [usize; 2],
        padded: [usize; 2],
    ) -> StoredBlockEncodingBuilder<BlockMetadata> {
        StoredBlockEncodingBuilder {
            u: self.u,
            pi_left: self.pi_left,
            pi_right: self.pi_right,
            state: BlockMetadata {
                alpha,
                original,
                padded,
            },
        }
    }
}
impl StoredBlockEncodingBuilder<BlockMetadata> {
    /// # Errors
    /// Rejects inconsistent shapes/metadata, nonfinite data and aggregate storage limits.
    pub fn build(self, policy: IoPolicy) -> Result<StoredBlockEncoding> {
        let block = StoredBlockEncoding {
            u: self.u,
            pi_left: self.pi_left,
            pi_right: self.pi_right,
            alpha: self.state.alpha,
            original: self.state.original,
            padded: self.state.padded,
        };
        validate_block(&block, policy)?;
        Ok(block)
    }
}
fn validate_block(block: &StoredBlockEncoding, policy: IoPolicy) -> Result<()> {
    let [original_rows, original_cols] = block.original;
    let [padded_rows, padded_cols] = block.padded;
    dimensions(original_rows, original_cols, policy)?;
    dimensions(padded_rows, padded_cols, policy)?;
    let dimension = block.u.nrows();
    let canonical = padded_rows
        .checked_add(padded_cols)
        .ok_or(Error::Budget("block canonical dimension"))?;
    if !block.alpha.is_finite()
        || block.alpha <= 0.
        || original_rows > padded_rows
        || original_cols > padded_cols
        || dimension != block.u.ncols()
        || !dimension.is_power_of_two()
        || dimension < canonical
        || block.pi_left.shape() != (dimension, padded_rows)
        || block.pi_right.shape() != (dimension, padded_cols)
    {
        return Err(Error::Format(
            "inconsistent block encoding shapes or metadata",
        ));
    }
    let count = [&block.u, &block.pi_left, &block.pi_right]
        .into_iter()
        .try_fold(0usize, |total, matrix| {
            let retained = usize::try_from(matrix.col_stride())
                .ok()
                .and_then(|stride| stride.checked_mul(matrix.ncols()))
                .ok_or(Error::Budget("matrix retained capacity"))?;
            let logical = dense_storage_count(matrix.nrows(), matrix.ncols(), policy)?;
            retained
                .max(logical)
                .checked_add(total)
                .ok_or(Error::Budget("block aggregate storage"))
        })?;
    policy.check(count, 3)?;
    for matrix in [&block.u, &block.pi_left, &block.pi_right] {
        for row in 0..matrix.nrows() {
            for col in 0..matrix.ncols() {
                finite(matrix[(row, col)])?;
            }
        }
    }
    Ok(())
}
fn complex_wire(
    values: impl IntoIterator<Item = Complex64>,
    count: usize,
    policy: IoPolicy,
) -> Result<Vec<H5ppComplex64>> {
    policy.check(count, 2)?;
    let mut wire = Vec::new();
    wire.try_reserve_exact(count)
        .map_err(|_| Error::Budget("HDF5 output buffer"))?;
    for value in values {
        let value = finite(value)?;
        wire.push(H5ppComplex64 {
            real: value.re,
            imag: value.im,
        });
    }
    if wire.len() != count {
        return Err(Error::Format("HDF5 output value count"));
    }
    Ok(wire)
}
fn write_integer(location: &Location, name: &str, value: usize) -> Result<()> {
    location
        .new_attr::<i64>()
        .create(name)?
        .write_scalar(&i64::try_from(value).map_err(|_| Error::Budget("HDF5 integer metadata"))?)?;
    Ok(())
}
fn write_complex_dataset(
    file: &File,
    path: &str,
    matrix: MatRef<'_, Complex64>,
    policy: IoPolicy,
) -> Result<()> {
    let count = matrix
        .nrows()
        .checked_mul(matrix.ncols())
        .ok_or(Error::Budget("HDF5 matrix dimensions"))?;
    let wire = complex_wire(
        (0..matrix.nrows()).flat_map(|row| (0..matrix.ncols()).map(move |col| matrix[(row, col)])),
        count,
        policy,
    )?;
    file.new_dataset::<H5ppComplex64>()
        .shape(matrix.shape())
        .create(path)?
        .write_raw(&wire)?;
    Ok(())
}
/// Replace a file with canonical complex128 `/state/vector` and dtype/length metadata.
/// Input admission finishes before opening the output file.
/// # Errors
/// Rejects shape, finite-value and storage limits, or reports HDF5 write errors.
pub fn write_state_vector(
    path: impl AsRef<Path>,
    values: &[Complex64],
    policy: IoPolicy,
) -> Result<()> {
    if values.is_empty() || values.len() > policy.max_dimension {
        return Err(Error::Format("state vector length"));
    }
    let wire = complex_wire(values.iter().copied(), values.len(), policy)?;
    let file = File::create(path)?;
    file.create_group("/state")?;
    let dataset = file
        .new_dataset::<H5ppComplex64>()
        .shape(values.len())
        .create("/state/vector")?;
    dataset.write_raw(&wire)?;
    write_integer(&dataset, "length", values.len())?;
    let dtype =
        FixedAscii::<16>::from_ascii("complex128").map_err(|_| Error::Format("dtype attribute"))?;
    dataset
        .new_attr::<FixedAscii<16>>()
        .create("dtype")?
        .write_scalar(&dtype)?;
    file.flush()?;
    Ok(())
}
/// Replace a file with canonical `U`, `PiL`, `PiR` and scale/dimension metadata.
/// No matrix is conjugated or transposed; datasets use logical row-major order.
/// # Errors
/// Rejects invalid data or aggregate storage limits before opening the output, or reports write errors.
pub fn write_block_encoding(
    path: impl AsRef<Path>,
    block: &StoredBlockEncoding,
    policy: IoPolicy,
) -> Result<()> {
    validate_block(block, policy)?;
    let file = File::create(path)?;
    let group = file.create_group("/block_encoding")?;
    write_complex_dataset(&file, "/block_encoding/U", block.u(), policy)?;
    write_complex_dataset(&file, "/block_encoding/PiL", block.pi_left(), policy)?;
    write_complex_dataset(&file, "/block_encoding/PiR", block.pi_right(), policy)?;
    group
        .new_attr::<f64>()
        .create("alpha")?
        .write_scalar(&block.alpha)?;
    let [original_rows, original_cols] = block.original;
    let [padded_rows, padded_cols] = block.padded;
    for (name, value) in [
        ("original_rows", original_rows),
        ("original_cols", original_cols),
        ("padded_rows", padded_rows),
        ("padded_cols", padded_cols),
    ] {
        write_integer(&group, name, value)?;
    }
    file.flush()?;
    Ok(())
}

fn bounded_text(value: &[u8]) -> Result<String> {
    if value.len() > 64 {
        return Err(Error::Format("oversized text attribute"));
    }
    Ok(std::str::from_utf8(value)
        .map_err(|_| Error::Format("non-UTF8 text attribute"))?
        .to_ascii_lowercase())
}

/// Replace a file with canonical complex128 `/matrix/dense` and shape metadata.
/// All dimensions, finite entries and allocation limits are admitted before file creation.
/// # Errors
/// Rejects shape, nonfinite values and storage limits, or reports HDF5 write errors.
pub fn write_matrix(
    path: impl AsRef<Path>,
    matrix: MatRef<'_, Complex64>,
    policy: IoPolicy,
) -> Result<()> {
    dimensions(matrix.nrows(), matrix.ncols(), policy)?;
    let count = matrix
        .nrows()
        .checked_mul(matrix.ncols())
        .ok_or(Error::Budget("HDF5 matrix dimensions"))?;
    let wire = complex_wire(
        (0..matrix.nrows()).flat_map(|r| (0..matrix.ncols()).map(move |c| matrix[(r, c)])),
        count,
        policy,
    )?;
    let shape = [
        u64::try_from(matrix.nrows()).map_err(|_| Error::Budget("matrix rows"))?,
        u64::try_from(matrix.ncols()).map_err(|_| Error::Budget("matrix columns"))?,
    ];
    let file = File::create(path)?;
    let group = file.create_group("/matrix")?;
    group
        .new_attr::<u64>()
        .shape(2)
        .create("shape")?
        .write_raw(&shape)?;
    file.new_dataset::<H5ppComplex64>()
        .shape(matrix.shape())
        .create("/matrix/dense")?
        .write_raw(&wire)?;
    file.flush()?;
    Ok(())
}
