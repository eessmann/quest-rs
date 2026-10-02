# Historical allocation fixture

`run.py` and `main.rs` reproduce the September 28 consolidation measurements.
Use the historical checkout identified by that record. The fixture intentionally
retains its original `quest-circuit` imports; the October static-architecture
migration removed that package. It is not a current consumer example or a
compatibility layer. Current measurements use a separately versioned fixture.
