#![cfg(feature = "hdf5")]
use googletest::prelude::*;
use hdf5_metno::{
    File, H5Type, Location,
    types::{FixedAscii, VarLenUnicode},
};
use quest_qsvt_io::{
    Complex64, IoPolicy,
    hdf5::{self, Hdf5Input, StoredBlockEncoding},
};
use std::str::FromStr;
fn fixed(location: &Location, name: &str, value: &str) -> googletest::Result<()> {
    let value = FixedAscii::<32>::from_ascii(value)?;
    location
        .new_attr::<FixedAscii<32>>()
        .create(name)?
        .write_scalar(&value)?;
    Ok(())
}
fn shape(group: &Location, rows: i64, cols: i64) -> googletest::Result<()> {
    group
        .new_attr::<i64>()
        .shape(2)
        .create("shape")?
        .write_raw(&[rows, cols])?;
    Ok(())
}
#[derive(Clone, Copy, H5Type)]
#[repr(C)]
struct CppComplex {
    real: f64,
    imag: f64,
}
#[gtest]
fn dense_rectangular_preserves_h5py_and_cpp_complex_order() -> googletest::Result<()> {
    let directory = tempfile::tempdir()?;
    for cpp in [false, true] {
        let path = directory
            .path()
            .join(if cpp { "cpp.h5" } else { "h5py.h5" });
        let file = File::create(&path)?;
        let group = file.create_group("matrix")?;
        shape(&group, 2, 3)?;
        fixed(&group, "format", "dense")?;
        fixed(&group, "dtype", "complex128")?;
        let data = [
            Complex64::new(1., 0.),
            Complex64::new(0., 2.),
            Complex64::new(3., -4.),
            Complex64::new(5., 6.),
            Complex64::new(7., 0.),
            Complex64::new(-8., 9.),
        ];
        if cpp {
            let raw: Vec<_> = data
                .iter()
                .map(|x| CppComplex {
                    real: x.re,
                    imag: x.im,
                })
                .collect();
            group
                .new_dataset::<CppComplex>()
                .shape((2, 3))
                .create("dense")?
                .write_raw(&raw)?;
        } else {
            group
                .new_dataset::<Complex64>()
                .shape((2, 3))
                .create("dense")?
                .write_raw(&data)?;
        }
        drop(file);
        let matrix = Hdf5Input::builder(&path)
            .policy(IoPolicy::default())
            .open()?
            .read_matrix()?;
        verify_that!(matrix.dimensions(), eq([2, 3]))?;
        let dense = matrix.into_dense(IoPolicy::default())?;
        verify_that!(dense[(0, 1)], eq(Complex64::new(0., 2.)))?;
        verify_that!(dense[(1, 0)], eq(Complex64::new(5., 6.)))?;
        verify_that!(dense[(1, 2)], eq(Complex64::new(-8., 9.)))?;
    }
    Ok(())
}
#[gtest]
fn sparse_one_based_duplicates_sum_in_declared_csr_and_csc_order() -> googletest::Result<()> {
    let directory = tempfile::tempdir()?;
    for csc in [false, true] {
        let path = directory.path().join(if csc { "csc.h5" } else { "csr.h5" });
        let file = File::create(&path)?;
        let group = file.create_group("matrix")?;
        shape(&group, 2, 3)?;
        fixed(&group, "format", if csc { "csc" } else { "csr" })?;
        fixed(&group, "dtype", "complex128")?;
        group
            .new_attr::<i32>()
            .create("index_base")?
            .write_scalar(&1)?;
        let values = [
            Complex64::new(1., 2.),
            Complex64::new(3., -1.),
            Complex64::new(0., 5.),
        ];
        group
            .new_dataset::<Complex64>()
            .shape(3)
            .create("data")?
            .write_raw(&values)?;
        group
            .new_dataset::<i64>()
            .shape(3)
            .create("indices")?
            .write_raw(if csc { &[1_i64, 1, 2] } else { &[2_i64, 2, 3] })?;
        let pointers: &[i64] = if csc { &[1, 1, 3, 4] } else { &[1, 3, 4] };
        group
            .new_dataset::<i64>()
            .shape(pointers.len())
            .create("indptr")?
            .write_raw(pointers)?;
        drop(file);
        let matrix = hdf5::read_matrix(&path, IoPolicy::default())?;
        verify_that!(matrix.sparse().is_some(), eq(true))?;
        let dense = matrix.into_dense(IoPolicy::default())?;
        verify_that!(dense[(0, 1)], eq(Complex64::new(4., 1.)))?;
        verify_that!(dense[(1, 2)], eq(Complex64::new(0., 5.)))?;
    }
    Ok(())
}
#[gtest]
fn state_roundtrip_and_variable_string_attributes_preserve_imaginary_values()
-> googletest::Result<()> {
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("state.h5");
    let values = [Complex64::new(0., 1.), Complex64::new(-0.25, 0.75)];
    hdf5::write_state_vector(&path, &values, IoPolicy::default())?;
    verify_that!(
        hdf5::read_state_vector(&path, IoPolicy::default())?,
        eq(&values.to_vec())
    )?;
    let second = directory.path().join("unicode.h5");
    let file = File::create(&second)?;
    file.create_group("state")?;
    let dataset = file
        .new_dataset::<Complex64>()
        .shape(2)
        .create("state/vector")?;
    dataset.write_raw(&values)?;
    dataset
        .new_attr::<i64>()
        .create("length")?
        .write_scalar(&2)?;
    dataset
        .new_attr::<VarLenUnicode>()
        .create("dtype")?
        .write_scalar(&VarLenUnicode::from_str("complex128")?)?;
    drop(file);
    verify_that!(
        hdf5::read_state_vector(&second, IoPolicy::default())?,
        eq(&values.to_vec())
    )?;
    Ok(())
}
#[gtest]
fn block_encoding_roundtrip_preserves_isometry_columns_and_metadata() -> googletest::Result<()> {
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("block.h5");
    let u = faer::Mat::from_fn(4, 4, |r, c| {
        Complex64::new(f64::from(r == c), if r == 0 && c == 3 { 0.4 } else { 0. })
    });
    let left = faer::Mat::from_fn(4, 1, |r, _| Complex64::new(0., f64::from(r == 2)));
    let right = faer::Mat::from_fn(4, 2, |r, c| Complex64::new(f64::from(r == c), 0.));
    let block = StoredBlockEncoding::builder(u, left, right)
        .metadata(2.5, [1, 2], [1, 2])
        .build(IoPolicy::default())?;
    hdf5::write_block_encoding(&path, &block, IoPolicy::default())?;
    let read = hdf5::read_block_encoding(&path, IoPolicy::default())?;
    verify_that!(read.u()[(0, 3)], eq(Complex64::new(0., 0.4)))?;
    verify_that!(read.pi_left()[(2, 0)], eq(Complex64::new(0., 1.)))?;
    verify_that!(read.pi_right().shape(), eq((4, 2)))?;
    verify_that!(read.alpha(), eq(2.5))?;
    verify_that!(read.original_dimensions(), eq([1, 2]))?;
    verify_that!(read.padded_dimensions(), eq([1, 2]))?;
    let tiny = IoPolicy {
        max_bytes: 64,
        ..IoPolicy::default()
    };
    verify_that!(hdf5::read_block_encoding(&path, tiny).is_err(), eq(true))?;
    Ok(())
}
#[gtest]
fn invalid_dtype_shape_nonfinite_and_budget_are_rejected_before_output_replacement()
-> googletest::Result<()> {
    let directory = tempfile::tempdir()?;
    for (case, tag, rows, cols) in [
        (0, "potato", 2, 2),
        (1, "float64", 2, 3),
        (2, "complex128", 2, 2),
    ] {
        let path = directory.path().join(format!("invalid{case}.h5"));
        let file = File::create(&path)?;
        let group = file.create_group("matrix")?;
        shape(&group, rows, cols)?;
        fixed(&group, "format", "dense")?;
        fixed(&group, "dtype", tag)?;
        group
            .new_dataset::<f64>()
            .shape((2, 2))
            .create("dense")?
            .write_raw(&[1., 0., 0., 1.])?;
        drop(file);
        verify_that!(
            hdf5::read_matrix(&path, IoPolicy::default()).is_err(),
            eq(true)
        )?;
    }
    let path = directory.path().join("state.h5");
    std::fs::write(&path, b"preserve")?;
    verify_that!(
        hdf5::write_state_vector(&path, &[Complex64::new(f64::NAN, 0.)], IoPolicy::default())
            .is_err(),
        eq(true)
    )?;
    verify_that!(std::fs::read(&path)?, eq(&b"preserve".to_vec()))?;
    let tiny = IoPolicy {
        max_bytes: 1,
        ..IoPolicy::default()
    };
    verify_that!(
        hdf5::write_state_vector(&path, &[Complex64::new(1., 0.)], tiny).is_err(),
        eq(true)
    )?;
    verify_that!(std::fs::read(&path)?, eq(&b"preserve".to_vec()))?;
    Ok(())
}

#[gtest]
fn unsupported_numeric_storage_nonfinite_input_and_bad_sparse_indices_fail()
-> googletest::Result<()> {
    let directory = tempfile::tempdir()?;
    for integer in [false, true] {
        let path = directory.path().join(if integer {
            "integer.h5"
        } else {
            "nonfinite.h5"
        });
        let file = File::create(&path)?;
        let group = file.create_group("matrix")?;
        shape(&group, 1, 2)?;
        fixed(&group, "format", "dense")?;
        if integer {
            group
                .new_dataset::<i32>()
                .shape((1, 2))
                .create("dense")?
                .write_raw(&[1, 2])?;
        } else {
            group
                .new_dataset::<f32>()
                .shape((1, 2))
                .create("dense")?
                .write_raw(&[1., f32::INFINITY])?;
        }
        drop(file);
        verify_that!(
            hdf5::read_matrix(&path, IoPolicy::default()).is_err(),
            eq(true)
        )?;
    }
    let path = directory.path().join("bad_indices.h5");
    let file = File::create(&path)?;
    let group = file.create_group("matrix")?;
    shape(&group, 2, 2)?;
    fixed(&group, "format", "csr")?;
    group
        .new_dataset::<f64>()
        .shape(1)
        .create("data")?
        .write_raw(&[1.])?;
    group
        .new_dataset::<i64>()
        .shape(1)
        .create("indices")?
        .write_raw(&[-1])?;
    group
        .new_dataset::<i64>()
        .shape(3)
        .create("indptr")?
        .write_raw(&[0, 1, 1])?;
    drop(file);
    verify_that!(
        hdf5::read_matrix(&path, IoPolicy::default()).is_err(),
        eq(true)
    )?;
    Ok(())
}
