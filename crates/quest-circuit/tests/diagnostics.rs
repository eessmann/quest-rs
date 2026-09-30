#![cfg(feature = "codespan-reporting")]
use googletest::{Result, prelude::*};
#[allow(unused_imports)]
use quest_circuit::prelude::*;
use quest_circuit::{Error, SourceSpan};

#[gtest]
fn source_renderer_labels_original_text_and_rejects_invalid_utf8_ranges() -> Result<()> {
    let span = SourceSpan::new("bell.qasm", 3, 4)?;
    let diagnostic = Error::InvalidId.render_source(&span, "cx q, unknown;")?;
    expect_true!(diagnostic.contains("bell.qasm:1:4"));
    expect_true!(diagnostic.contains("cx q, unknown;"));
    expect_true!(diagnostic.contains("identifier does not belong"));
    let invalid = SourceSpan::new("unicode.qasm", 1, 2)?;
    expect_true!(Error::InvalidId.render_source(&invalid, "λ").is_err());
    expect_true!(Error::InvalidId.render_source(&span, "x").is_err());
    Ok(())
}
