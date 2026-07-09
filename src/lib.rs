pub mod core;
pub use core::environment::QuESTEnvironment;
pub use core::register::QuantumRegister;

pub fn add(left: u64, right: u64) -> u64 {
    left + right
}

#[cfg(test)]
mod tests {
    use super::*;
    use googletest::prelude::*;

    #[gtest]
    fn it_works() {
        expect_that!(add(2, 2), eq(4));
    }
}
