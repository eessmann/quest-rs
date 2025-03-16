#[cxx::bridge]
pub mod ffi {
    // Types
    unsafe extern "C++" {
        // Opaque QuEST types
        type Qureg;

        // Common type
        type Quest_Complex = crate::types::ffi::Quest_Complex;
    }

    // Qureg
    #[namespace = "quest_sys"]
    unsafe extern "C++" {
        include!("qureg.hpp");
        // Qureg creation and destruction
        fn createQureg(numQubits: i32) -> UniquePtr<Qureg>;
        fn createDensityQureg(numQubits: i32) -> UniquePtr<Qureg>;
        fn createForcedQureg(numQubits: i32) -> UniquePtr<Qureg>;
        fn createForcedDensityQureg(numQubits: i32) -> UniquePtr<Qureg>;
        fn createCustomQureg(
            numQubits: i32,
            isDensMatr: i32,
            useDistrib: i32,
            useGpuAccel: i32,
            useMultithread: i32,
        ) -> UniquePtr<Qureg>;
        fn createCloneQureg(qureg: &Qureg) -> UniquePtr<Qureg>;
        fn destroyQureg(qureg: Pin<&mut Qureg>);

        // Qureg reporting
        fn reportQuregParams(qureg: &Qureg);
        fn reportQureg(qureg: &Qureg);

        // Qureg synchronization
        fn syncQuregToGpu(qureg: Pin<&mut Qureg>);
        fn syncQuregFromGpu(qureg: Pin<&mut Qureg>);
        fn syncSubQuregToGpu(qureg: Pin<&mut Qureg>, localStartInd: i64, numLocalAmps: i64);
        fn syncSubQuregFromGpu(qureg: Pin<&mut Qureg>, localStartInd: i64, numLocalAmps: i64);

        // Qureg amplitude access
        fn getQuregAmps(qureg: Pin<&mut Qureg>, startInd: i64, numAmps: i64) -> Vec<Quest_Complex>;
        fn getDensityQuregAmps_flatten(
            qureg: Pin<&mut Qureg>,
            startRow: i64,
            startCol: i64,
            numRows: i64,
            numCols: i64,
        ) -> Vec<Quest_Complex>;
        fn getQuregAmp(qureg: Pin<&mut Qureg>, index: i64) -> Quest_Complex;
        fn getDensityQuregAmp(qureg: Pin<&mut Qureg>, row: i64, column: i64) -> Quest_Complex;
    }
}
