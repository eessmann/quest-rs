#![feature(const_trait_impl, const_ops, const_destruct)]
#![forbid(unsafe_code)]
//! Maintained exact algebra and backend-neutral ordered mathematical expressions.
pub use dashu_ratio::RBig;
pub mod arithmetic;
mod dyadic;
pub mod dynamic;
pub mod exact;
pub mod geometry;
pub mod multivariate;
pub mod scalar;
mod scope;
pub mod typed;
