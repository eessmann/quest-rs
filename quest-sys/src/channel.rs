#[cxx::bridge]
pub mod ffi {
    // Types
    unsafe extern "C++" {
        // Opaque QuEST types
        type KrausMap;
        type SuperOp;

        // Common type
        type Quest_Complex = crate::types::ffi::Quest_Complex;
    }

    // Channels
    #[namespace = "quest_sys"]
    unsafe extern "C++" {
        include!("channels.hpp");

        fn createKrausMap(numQubits: i32, numOperators: i32) -> UniquePtr<KrausMap>;
        fn syncKrausMap(map: Pin<&mut KrausMap>);
        fn destroyKrausMap(map: Pin<&mut KrausMap>);
        fn reportKrausMap(map: &KrausMap);

        fn createSuperOp(numQubuts: i32) -> UniquePtr<SuperOp>;
        fn syncSuperOp(map: Pin<&mut SuperOp>);
        fn destroySuperOp(map: Pin<&mut SuperOp>);
        fn reportSuperOp(map: &SuperOp);

        fn setKrausMap(map: Pin<&mut KrausMap>, matrices: &[&[&[Quest_Complex]]]);
        fn setSuperOp(map: Pin<&mut SuperOp>, matrix: &[&[Quest_Complex]]);
        fn createInlineKrausMap(
            numQubits: i32,
            numOperators: i32,
            matrices: &[&[&[Quest_Complex]]],
        ) -> UniquePtr<KrausMap>;
        fn createInlineSuperOp(numQubits: i32, matrix: &[&[Quest_Complex]]) -> UniquePtr<SuperOp>;
    }
}
