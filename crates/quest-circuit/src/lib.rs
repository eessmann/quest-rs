#![forbid(unsafe_code)]
//! Public construction, compilation, and interchange facade.
//!
//! Semantic ownership lives in `quest-language`; native-independent compiler
//! passes live in `quest-compile`. Native resource ownership lives in `quest`.
pub use quest_compile::*;
extern crate self as quest_circuit;
#[cfg(feature = "macros")]
pub use quest_macros::{circuit, circuit_file};
