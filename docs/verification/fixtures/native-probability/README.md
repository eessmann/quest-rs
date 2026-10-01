# Native probability witness

This fixture reproduces the witness used for the reviewed QuEST 4.3 fork.
Select the matching installed package and source checkout when configuring:

```sh
cmake -S docs/verification/fixtures/native-probability \
  -B target/native-probability \
  -DCMAKE_PREFIX_PATH=/path/to/installed/quest/lib64/cmake/QuEST \
  -DQUEST_SOURCE_DIR=/path/to/QuEST
cmake --build target/native-probability
```

Replace the placeholder paths with your package and source locations.
The fixture instruments the calculations translation unit
with the shift sanitizer and traps on the fault; it does not instrument all of
QuEST or the Rust bridge.

The `witness` argument is the target count. The recorded one-target run exits
normally; the 64-target run traps with `SIGILL`. Core dumps were disabled during
the recorded runs. This intentionally demonstrates the native vector overload's
pre-validation fault. The fixed Rust bridge rejects that request before entering
the allocation-performing overload.
