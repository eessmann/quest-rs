#[cxx::bridge]
pub mod ffi {
    // Types
    unsafe extern "C++" {
        // Common type
        type Quest_Complex = crate::types::ffi::Quest_Complex;
    }

    // Debug
    #[namespace = "quest_sys"]
    unsafe extern "C++" {
        include!("debug.hpp");
        // Random number generators
        fn setSeeds(seeds: &[u32]);
        fn setSeedsToDefault();
        fn getSeeds() -> Vec<u32>;

        // Validation
        fn invalidQuESTInputError(msg: String, func: String);
        fn setValidationOn();
        fn setValidationOff();
        fn setValidationEpsilonToDefault();
        fn setValidationEpsilon(eps: f64);
        fn getValidationEpsilon() -> f64;

        // Reporting
        fn setMaxNumReportedItems(numRows: i64, numCols: i64);
        fn setMaxNumReportedSigFigs(numSigFigs: i32);

        // GPU cache
        fn getGpuCacheSize() -> i64;
        fn clearGpuCache();

        // Environment
        fn getEnvironmentString() -> String;
    }
}
