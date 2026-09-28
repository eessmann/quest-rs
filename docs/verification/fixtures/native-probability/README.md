# Native probability witness

This is the exact host-specific fixture used for the reviewed QuEST 4.3 fork.
Its CMake file records the original installed/source paths. Adjust those paths
to reproduce on another host. It instruments the calculations translation unit
with the shift sanitizer and traps on the fault; it does not instrument all of
QuEST or the Rust bridge.

The `witness` argument is the target count. The recorded one-target run exits
normally; the 64-target run traps with `SIGILL`. Core dumps were disabled during
the recorded runs. This intentionally demonstrates the native vector overload's
pre-validation fault. The fixed Rust bridge rejects that request before entering
the allocation-performing overload.
