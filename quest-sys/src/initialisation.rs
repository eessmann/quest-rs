#[cxx::bridge]
pub mod ffi {
    // Types
    unsafe extern "C++" {
        // Opaque QuEST types
        type Qureg;
        type PauliStrSum;

        // Common type
        type Quest_Complex = crate::types::ffi::Quest_Complex;
    }

    // Initialisation
    #[namespace = "quest_sys"]
    unsafe extern "C++" {
        include!("initialisation.hpp");
        // State initialization
        fn initBlankState(qureg: Pin<&mut Qureg>);
        fn initZeroState(qureg: Pin<&mut Qureg>);
        fn initPlusState(qureg: Pin<&mut Qureg>);
        fn initPureState(qureg: Pin<&mut Qureg>, pure: Pin<&mut Qureg>);
        fn initClassicalState(qureg: Pin<&mut Qureg>, stateInd: i64);
        fn initDebugState(qureg: Pin<&mut Qureg>);
        fn initArbitraryPureState(qureg: Pin<&mut Qureg>, amps: &[Quest_Complex]);
        fn initRandomPureState(qureg: Pin<&mut Qureg>);
        fn initRandomMixedState(qureg: Pin<&mut Qureg>, numPureStates: i64);

        // Setting amplitudes
        fn setQuregAmps(qureg: Pin<&mut Qureg>, startInd: i64, amps: &[Quest_Complex]);
        fn setDensityQuregAmps(
            qureg: Pin<&mut Qureg>,
            startRow: i64,
            startCol: i64,
            amps: &[&[Quest_Complex]],
        );
        fn setDensityQuregFlatAmps(qureg: Pin<&mut Qureg>, startInd: i64, amps: &[Quest_Complex]);

        // Qureg manipulation
        fn setQuregToClone(targetQureg: Pin<&mut Qureg>, copyQureg: &Qureg);
        fn setQuregToSuperposition(
            facOut: Quest_Complex,
            out: Pin<&mut Qureg>,
            fac1: Quest_Complex,
            qureg1: &Qureg,
            fac2: Quest_Complex,
            qureg2: &Qureg,
        );
        fn setQuregToRenormalized(qureg: Pin<&mut Qureg>) -> f64;
        fn setQuregToPauliStrSum(qureg: Pin<&mut Qureg>, sum: &PauliStrSum);
    }
}
