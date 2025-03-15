#[cxx::bridge]
mod ffi {
    // Shared struct for complex numbers
    #[derive(Debug, Clone, Copy)]
    struct Quest_Complex {
        re: f64,
        im: f64,
    }

    unsafe extern "C++" {
        include!("bindings.h");
        type Quest_Complex;
    }
}

// Re-export types and functions for convenience
pub use ffi::*;

/// Convenience function to create a quest_complex
pub fn complex(re: f64, im: f64) -> Quest_Complex {
    Quest_Complex { re, im }
}

/// Pauli operator enum
#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pauli {
    I = 0,
    X = 1,
    Y = 2,
    Z = 3,
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_complex() {
        let c = complex(1.0, 2.0);
        assert_eq!(c.re, 1.0);
        assert_eq!(c.im, 2.0);
    }
}
