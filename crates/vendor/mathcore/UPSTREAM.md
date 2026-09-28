# Upstream provenance

This directory was imported from the official crates.io `mathcore` 0.3.1
archive at `https://static.crates.io/crates/mathcore/mathcore-0.3.1.crate`.
The SHA-256 of the downloaded archive is
`367d848a146b75a63af4fc2eafd6791327a662ccb4ff5965220fcfa7dfa1c96f`.
The official crates.io sparse index record at
`https://index.crates.io/ma/th/mathcore` lists the same `cksum` for version
`0.3.1`; the same archive bytes were also obtained through the crates.io
download redirect. The archive's `.cargo_vcs_info.json` identifies upstream
commit `d13a333a6c69a231f210cdc3fb1c4843ee7bd2eb`.
The upstream MIT license is preserved in `LICENSE`.

The baseline import is the unmodified archive contents plus this provenance note.
Fork changes are prepared separately from that baseline; the delivery report
records their signed-commit status.

## Maintained quest fork

The local package is `quest-mathcore` version `0.3.1-quest.1`, exposed under
the Rust dependency alias `mathcore`. The quest-rs maintainers own these patches;
they are not upstream mathcore releases. The verified archive baseline is tracked independently from the exact-domain
implementation and its regressions.

The `exact` feature adds immutable, budgeted affine expressions over normalized
arbitrary-precision rational coefficients: a rational constant, a distinguished
pi coefficient, and sorted owner-scoped symbol terms. Construction, arithmetic,
simultaneous substitution and export are fallible. No floating-point zero test,
transcendental evaluator, expression parser or legacy simplifier participates
in this domain.

Existing approximate CAS modules remain behind `legacy`; their dependencies are
optional. The legacy rational implementation retains its original num-bigint
version through an explicit package alias. The exact domain uses the quest
workspace's newer num-bigint version. `quest-symbolic` selects only `exact` with
default features disabled. The fork has its own workspace and lockfile so its
legacy compatibility tests can run separately from quest-rs.

This patch does not make the legacy CAS exact or repair its unused algorithms.
Independent quest affine equality, cyclotomic reconstruction and interval
certification remain outside this fork. Review evidence and validation commands
are recorded in the optimization-roadmap implementation artifacts and delivery
report in quest-rs.
