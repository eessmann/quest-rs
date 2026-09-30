# Pinned newsynth executable oracle

These four ASCII gate words were generated on 2026-09-29 by the unmodified
algorithm in newsynth **0.4.1.0**, built with GHC 9.10.3 on aarch64-darwin.
No newsynth implementation source is included in this crate or translated into
its Rust implementation. Upstream source is GPL licensed; these files are
executable outputs used solely as independent mathematical fixtures.

- Source: https://hackage.haskell.org/package/newsynth-0.4.1.0/newsynth-0.4.1.0.tar.gz
- Source SHA256: `9476268de585ef3592ee8896beafa067d880c7330ebbc9ec4e0f10c8ceab172d`
- Nixpkgs revision: `6774f7bc253789b113a4f39285dc0fa100abeacc`
- Dependencies: `random-1.1`, `fixedprec-0.2.2.2`; Cabal dependency bounds for
  random, fixedprec and newsynth were relaxed using `doJailbreak` to build with
  GHC 9.10.3. No algorithm source was edited.
- Executable: `/nix/store/zg8yx7f1gdgmn5sg2lzmzx8n6w8bsv7m-newsynth-0.4.1.0/bin/gridsynth`

Commands, with no `--phase` option (global phase is retained):

```sh
gridsynth --epsilon=5e-13 --rseed=0 pi/7 > newsynth-pi7.txt
gridsynth --epsilon=5e-13 --rseed=0 '1/3+pi/5' > newsynth-affine.txt
gridsynth --epsilon=5e-13 --rseed=0 pi/4 > newsynth-pi4.txt
gridsynth --epsilon=5e-13 --rseed=0 1.125 > newsynth-dyadic.txt
```

Newsynth prints a mathematical product, so tests reverse the word to obtain
chronological operations. Its operator-norm tolerance is set conservatively to
5e-13 so the independently recomputed Frobenius error can be certified at the
project's exact dyadic encoding of 1e-12. The tests check every fixture against
its exact target, reject an added scalar omega phase, then synthesize and
normalize its exact D[omega] matrix and compare full matrix equality.

The generated Rust words need not equal newsynth words or have identical
T-counts: the Rust enumerator uses rational LLL and bounded sphere enumeration.
Native Rust 1e-12 candidates are independently tested in `approximation.rs`.
