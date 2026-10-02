# quest-optimizer-protocol

Versioned JSON transport for one request and one response per optimizer child process. Version `3` envelopes carry a `u64` seed; the client rejects a response whose version or seed does not match. Requests select synthesis, ZX optimization, or capability discovery. Responses contain a candidate, a capability report, or a failure code and message.

Requests and responses are limited to 65,536 encoded bytes. The encoder checks its budget before extending its buffer; the decoder checks input length and rejects malformed or trailing JSON while retaining serde's nesting limit. Envelope and operation discriminants reject unknown fields. Sequence and target values are transport data: deserialization never constructs a mathematical certificate.

Synthesis transports an exact rational multiple of pi, an exact binary64 radian bit pattern, or an affine combination of rational radians and pi, plus the requested tolerance's binary64 bits. Exact integers use canonical decimal strings; rational pairs are reduced and have positive denominators. Quantum sequences retain ordered targets, signed controls, and explicit scalar eighth-root gates. A candidate's engine name and precision metadata are informational. The client performs independent mathematical admission before any replacement.

```sh
cargo test -p quest-optimizer-protocol --locked
```
