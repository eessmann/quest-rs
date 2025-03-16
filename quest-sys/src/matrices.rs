#[cxx::bridge]
pub mod ffi {
    // Types
    unsafe extern "C++" {
        // Opaque QuEST types
        type Qureg;
        type CompMatr;
        type CompMatr1;
        type CompMatr2;
        type DiagMatr;
        type DiagMatr1;
        type DiagMatr2;
        type FullStateDiagMatr;
        type PauliStrSum;

        // Common type
        type Quest_Complex = crate::types::ffi::Quest_Complex;
    }

    // Matrices
    #[namespace = "quest_sys"]
    unsafe extern "C++" {
        include!("matrices.hpp");
        // Matrix creation and destruction
        fn getCompMatr1(in_: &[&[Quest_Complex]]) -> UniquePtr<CompMatr1>;
        fn getCompMatr2(in_: &[&[Quest_Complex]]) -> UniquePtr<CompMatr2>;
        fn getDiagMatr1(in_: &[Quest_Complex]) -> UniquePtr<DiagMatr1>;
        fn getDiagMatr2(in_: &[Quest_Complex]) -> UniquePtr<DiagMatr2>;

        fn createCompMatr(numQubits: i32) -> UniquePtr<CompMatr>;
        fn createDiagMatr(numQubits: i32) -> UniquePtr<DiagMatr>;
        fn createFullStateDiagMatr(numQubits: i32) -> UniquePtr<FullStateDiagMatr>;
        fn createCustomFullStateDiagMatr(
            numQubits: i32,
            useDistrib: i32,
            useGpuAccel: i32,
        ) -> UniquePtr<FullStateDiagMatr>;

        fn destroyCompMatr(matrix: Pin<&mut CompMatr>);
        fn destroyDiagMatr(matrix: Pin<&mut DiagMatr>);
        fn destroyFullStateDiagMatr(matrix: Pin<&mut FullStateDiagMatr>);

        // Matrix synchronization
        fn syncCompMatr(matr: Pin<&mut CompMatr>);
        fn syncDiagMatr(matr: Pin<&mut DiagMatr>);
        fn syncFullStateDiagMatr(matr: Pin<&mut FullStateDiagMatr>);

        // Setting matrix values
        fn setCompMatr(out: Pin<&mut CompMatr>, in_: &[&[Quest_Complex]]);
        fn setDiagMatr(out: Pin<&mut DiagMatr>, in_: &[Quest_Complex]);
        fn setFullStateDiagMatr(
            out: Pin<&mut FullStateDiagMatr>,
            startInd: i64,
            in_: &[Quest_Complex],
        );

        // Special matrix creation
        fn createFullStateDiagMatrFromPauliStrSum(
            in_: &PauliStrSum,
        ) -> UniquePtr<FullStateDiagMatr>;
        fn setFullStateDiagMatrFromPauliStrSum(out: Pin<&mut FullStateDiagMatr>, in_: &PauliStrSum);

        // Matrix reporting
        fn reportCompMatr1(matrix: &CompMatr1);
        fn reportCompMatr2(matrix: &CompMatr2);
        fn reportCompMatr(matrix: &CompMatr);
        fn reportDiagMatr1(matrix: &DiagMatr1);
        fn reportDiagMatr2(matrix: &DiagMatr2);
        fn reportDiagMatr(matrix: &DiagMatr);
        fn reportFullStateDiagMatr(matr: &FullStateDiagMatr);
    }
}
