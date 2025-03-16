#[cxx::bridge]
pub mod ffi {
    // Share these types across all bridges
    #[derive(Debug, Clone, Copy)]
    pub struct Quest_Complex {
        pub re: f64,
        pub im: f64,
    }

    unsafe extern "C++" {
        include!("types.hpp");

        // Opaque QuEST types
        type Qureg;
        type CompMatr;
        type CompMatr1;
        type CompMatr2;
        type DiagMatr;
        type DiagMatr1;
        type DiagMatr2;
        type FullStateDiagMatr;
        type PauliStr;
        type PauliStrSum;
        type KrausMap;
        type SuperOp;
        type QuESTEnv;

        // Common type
        type Quest_Complex;
    }
}
