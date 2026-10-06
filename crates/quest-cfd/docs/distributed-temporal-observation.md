# Simulated joint inverse and temporal observation

`PreparedHistoryInverse::observe_temporal_postselected` composes an existing distributed inverse output, a `TemporalEncoding`, and a typed `GridPhysicalObservable`. It runs on the native CPU/MPI state-vector simulator and mutates the supplied register through unnormalized projectors and the complete temporal unitary. It executes no sampled quantum measurements.

The source identity is a nonzero caller provenance premise binding the physical chart, prepared observable, configuration grid and history coordinates. Matching dimensions and a matching integer do not prove that premise. Likewise, the method cannot prove that an arbitrarily supplied register is the corresponding inverse output. Tests deliberately supply known artificial states as circuit semantics fixtures, separately from an executed inverse.

## Admission and mutation boundary

Before any register mutation, every participant agrees the history semantics, source identities, physical scale, temporal construction identity, widths, limits, ranges and callback costs. The temporal system mask must equal the parent's system mask; existing inverse work bits must be sufficient for every temporal flag/color bit. Unsupported layouts and GPU execution reject. No extra register is silently allocated.

A two-pass bounded probability reduction validates every typed configuration observable value, even on zero-amplitude coordinates, along with its finite range and readout capacity. This also rejects a zero-norm original input. The temporal gate executor is allocated, and the complete replay is scanned for valid targets, signed controls, angles and admitted gate count. Owned/borrowed recipe payload and overlapping readout storage are admitted and reserved throughout the operation. A separate bounded scan records original workspace-failure and clean-system-padding mass. No full state, observable table or gate array is collected.

Commit then performs these operations, in order:

1. Project **all** inverse non-system bits to zero, without renormalization. Only now are the reused flag/color bits clean.
2. Replay the complete temporal unitary, including failure and padding sectors.
3. Project its flag/color workspace to zero, again without renormalization.
4. Reduce the diagonal only on output configuration rows `i<m`.

Recoverable metadata, budget, source or finite-data errors occur before step 1. After the first projector, any error or panic aborts the MPI job, including failures in native dispatch, callback replay, allocation or subsequent readout. No recoverable result can expose a partially completed projected state. Bounded MPI subprocess tests exercise error and panic aborts after an actual native projector, including a peer waiting in a subsequent collective.

An exactly zero binary64 projected mass skips the probability reducer's positive-norm division and returns zero numerical moments with no conditional mean or variance. No tolerance is used. `exact_zero_projected_branch` refers only to that binary64 mass comparison; it is not an exact-arithmetic null-state or physical-solution certificate. Floating cancellation can leave tiny gate remnants, and underflow is not ruled out by this flag.

## Probabilities and physical normalization

Let the original register mass be `M`, and let `z` be its inverse-success logical history amplitudes. The first projector also retains clean system padding; the rectangular temporal success block annihilates that padding mathematically. For temporal map `C` normalized by `alpha`, the final selected logical amplitudes are `C z / alpha`, up to floating execution error.

The report identifies the explicit temporal slab and left/interior/right side, along with the parent-history and temporal source/construction fingerprints. Thus one-sided traces at a shared physical time remain distinguishable.

The report keeps the following quantities separate:

- Original total mass, inverse logical-success mass/probability, workspace-failure mass and clean system-padding mass.
- Final projected total mass and selected configuration mass.
- Joint inverse-plus-temporal selected probability `selected_mass / M`, relative to the **original** input, not to the postprojected register.
- Conditional selected diagonal expectation and variance, computed from final amplitudes and their interference.
- Physical amplitude scale `parent_physical_scale * alpha`; selected squared norm and diagonal quadratic functional multiply numerical selected moments by the square of that scale.

The parent's existing probability reducer normalizes its probabilities by the register it receives. Its probabilities after projection are therefore not reused as original inverse/joint probabilities. Its unnormalized masses and moments are reused, and the extra `alpha²` is included in physical rescaling.

The result is a conditional KvN ensemble expectation under the caller's physical provenance premise. It is not the observable at the ensemble mean, a deterministic field-recovery certificate, or a resolution/convergence result. Fixed capture, grid, temporal, preparation, inverse and native floating biases remain outside any statistical epsilon. The simulator's observed joint probability is not automatically a certified lower bound for the sampling planner.

## Resource scope

`TemporalReadoutResources` separates:

- Previous replicated temporal-encoding preparation, physical snapshot preparation and interval-grid range preparation, per rank and aggregate. Those already-completed costs are recorded without pretending to execute them again.
- Two additional temporal source replay visits: preflight and actual gate execution.
- Native temporal gate, inverse-workspace projector, temporal-workspace projector and probability-query dispatch counts.
- Up to five complete bounded amplitude passes: two preflight reducer passes, one original-branch scan and two final reducer passes. An exactly zero projected mass can skip the last two; the receipt remains the admitted upper bound.
- Repeated callback work, additional retained/query bytes, maximum rank/node envelopes, readout/control transport and conservative native amplitude-exchange payload.

Work/transport values are conservative logical arithmetic and payload bounds, not measured CPU cycles, MPI packet overhead or measured network traffic. Native state, temporary native read buffers and executor control storage remain covered by the enclosing environment memory budget; source payload and its overlap are separately charged. The node envelope uses the parent's declared ranks-per-node policy, defaulting conservatively to all participants. Shared borrowed sources may be counted more than once conservatively.

The old inverse-only `sampling_cost` API remains unchanged. A complete experimental shot estimate must additionally charge this temporal unitary and its joint success selection on every attempted fresh preparation/inverse repetition. This simulation does not implement that repeated-shot experiment.

## Focused validation

The MPI fixture retains the complete one-coordinate BDM1 coarse cavity space and all three DG2 configuration nodes. A nonzero signed physical velocity-component probe is compared against the original full physical reconstruction. This is a small circuit/normalization fixture, not a resolved CFD claim.

Known complex DG1 histories include nonzero inverse failure, clean padding and dirty workspace. Every small final native amplitude is checked against direct complex interpolation. A separate actual prepared inverse of zero configuration dynamics has nonzero observable signal and a relative physical-functional comparison. Work, bytes, transport, gate count, rank-inconsistent identity and wrong-history metadata reject without changing any amplitudes. Zero original norm rejects before mutation. Opposite amplitudes test cancellation, and a pure failure input tests the exactly zero projected-mass branch.

A DG2 case with a negative interpolation weight compares all final native amplitudes to an independently specified complex quadratic polynomial in time. DG1 composition runs on MPI 1/2/4/8 and split communicators; DG2 runs on 1/2/4 and split communicators. These tiny checks do not certify a large inverse or physical convergence.
