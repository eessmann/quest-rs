# Persisted weighted transform attempt1 — saved-evidence independent audit

2026-10-06. **Approved as retained failure evidence only.** The saved source/build/runtime/raw identities and outcome arithmetic are internally consistent and match current bytes at this audit. No physical job, compiler, MPI launcher or source generator was rerun; no limits, scientific policies or production sources were changed. No publication was made. There is no N32 inverse execution or accuracy result.

## Build and execution identity

The four build/execution before/after snapshot JSONs are byte-identical, SHA256`f961a3adcf3006e3f7ab30bfe8f7b410505ee3fbb8734a985265bbbbf041a629`. Every one of970 listed source-file hashes is recomputed against current bytes. The aggregate uses relative UTF8 path + NUL + **raw32 digest bytes**, producing`b0d6dbe054576507fdbaa2b9777a1695fcf660ca565052a41402a923d38b39f5`. The serialized source manifest SHA is separately`21587e89a90d3281f22f3f633ac7bf5201aeb760574401cc182c08b11b2b7519`; these are different aggregate scopes/encodings and must not be conflated.

The970 files comprise an audited workspace superset: tracked/untracked crate files including Markdown, verification fixtures/data, Cargo manifests/lock, existing .cargo/toolchain/nextest configuration. This is not a claim that970 files were compiler inputs. Independently parsed373 compiler JSON records show230 distinct compiler-artifact package IDs, of which exactly16 local manifests match the attestation (including installed-native quest/quest-sys, qsvt-io, qsp, qsvt, and vendored MathCore; excluding CFD). The one consumer executable record has the recorded debug profile opt_level0/debuginfo1/assertions+overflow checks enabled and features mpi/qsvt/qsvt-io; final build-finished is success and build exit0. All compiler-artifact/build-log hashes match. Cached artifact reporting establishes the actual package set, not a hermetic compilation replay.

All14 combined consumer manifest entries (Cargo.lock, seven Rust files, method doc and Python/fixtures) match listed sizes/SHA, and the full manifest SHA is`a1a1a1edc4700731aa83249450a2257a6a839b4a8827c74a70ef00a7157f9e58`. The final reviewed seven Rust entries remain334f24477e6b09b2ae3c42b36852ae3386e37c5462824384ee3e42dabd8875ab; native owner's rust-source-fix-2 manifest SHA94933d5f27352ccd56430d38d70fa7761c5f38b0a0afa5fee40afb4147608921 binds those same entries. Initial/fix1 manifests remain historical.

All29 installed QuEST header/CMake/library file hashes and five resolved tool executable records match current bytes. rustc and cargo hashes identify the shared rustup launcher; actual rustc/cargo version output is saved separately, rather than presenting rustup SHA as an underlying compiler binary SHA. MPI wrapper/launcher and timeout executable identities are genuine. External registry contents, dependent shared libraries and arbitrary environment are not hashed, so this is no hermetic build claim. Private source-identity helper/wrapper and actual runner hashes also match.

The original pinned0555 executable, copied0500 campaign executable and Cargo's reported actual example executable all hash to`2bdc5ca75c018f9f5fe40b33d08050bb313f5b41b14901252ecaa4774426cf87`. Build attestation SHA`c2e27731293e8d8b1589235edfc24ac3ee9a44a37a63d4ecfe4c10749639825c` matches the execution/campaign receipts. Read-only verification streams hashes in bounded chunks; it does not execute those files.

## Actual retained outcome

Raw receipt SHA`5426cd73301c5489b87543d09e629d1da2920b0a45ad0608ce589046013f592b` matches the execution attestation. Its fixed seven requests are publish8, compile1, replay1/2/4/8 and split4. Exactly the first job launched and returned153=128+25; launcher text explicitly reports File size limit exceeded(signal25). It is process-failure, neither a timeout nor accuracy failure. Source/binary/build before/after flags and hashes are stable. Driver exit0 means it successfully retained a campaign outcome; it does not mean the publication or any solve succeeded.

- Captured stdout555 bytes, SHA395f87919c57f29ace9a205bbcea70e123a80def08586fb63da9f79bdb1e0049.
- Captured stderr0 bytes, empty SHAe3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855.
- Dataset directory has0 entries; immutable_inputs is null, no published bucket/phase/native receipt exists.
- Six dependent jobs are not-run with no child records: one compilation and all five replay layouts.
- Actual child2.168690227s <= driver2.271779712s <= outer execution2.502042485s, below180s/1500s time caps.
- Child caps remain AS2147483648, RLIMIT_FSIZE4194304 and180s; capture streams are both below4MiB. No cap was raised or scientific policy retried during this attempt.

The saved artifacts identify SIGXFSZ via the launcher, but do not name the offending file or establish MPI-internal allocation as its cause. RLIMIT_FSIZE applies to every inherited regular-file write, not solely the captured stdout/stderr files. Thus the small streams rule out their reaching4MiB, but do not rule out an internal/native/launcher file. Any causal diagnosis belongs to the separately approved minimal initialization fixtures. This audit supplies no memory-envelope acceptance, capacity scale or inverse signal/accuracy measurement.

## Reproducible audit evidence

Read-only script `<private-artifacts>/quest-persisted-weighted-attempt-1-independent-audit.py`, command `python3 -B <private-artifacts>/quest-persisted-weighted-attempt-1-independent-audit.py`, log and results JSON beside it. The script uses strict finite/duplicate JSON decoding, independently recomputes all970+29+5 file hashes and raw artifact hashes, derives the actual package set/profile from compiler records, recomputes the source aggregate and validates all request/cap/outcome/timing relationships. Companion ledger JSON inventories exact evidence hashes.

No actionable inconsistency found. Keep attempt1 immutable, classify it as incomplete publication/process failure, and obtain explicit review/authorization before any separately named policy correction or new campaign.
