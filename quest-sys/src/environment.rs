#[cxx::bridge]
pub mod ffi {
    // Types
    unsafe extern "C++" {
        // Opaque QuEST types
        type QuESTEnv;

        // Common type
        type Quest_Complex = crate::types::ffi::Quest_Complex;
    }

    // Environment management
    #[namespace = "quest_sys"]
    unsafe extern "C++" {
        include!("environment.hpp");
        fn initQuESTEnv();
        fn initCustomQuESTEnv(useDistrib: bool, useGpuAccel: bool, useMultithread: bool);
        fn finalizeQuESTEnv();
        fn syncQuESTEnv();
        fn reportQuESTEnv();
        fn isQuESTEnvInit() -> bool;
        fn getQuESTEnv() -> UniquePtr<QuESTEnv>;
    }
}
