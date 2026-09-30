//! Portable checked frontend templates. Serialized witnesses are never trusted.
use super::{CompileLimits, SemanticError, TypedModule};
use crate::{ssa, syntax};

#[derive(serde::Serialize, serde::Deserialize)]
struct Template {
    version: u32,
    syntax: syntax::Module,
    program: ssa::Program,
}

/// Encode admitted syntax and its checked executable graph at macro expansion.
/// # Errors
/// Rejects encoding failures and the default template storage budget.
pub fn encode(module: &TypedModule) -> Result<String, SemanticError> {
    let template = Template {
        version: 1,
        syntax: module.syntax.clone(),
        program: module.program.program().clone(),
    };
    let encoded = serde_json::to_string(&template)
        .map_err(|error| SemanticError::invalid(format!("template encoding: {error}")))?;
    if encoded.len() > CompileLimits::default().storage_bytes {
        return Err(SemanticError::budget("template encoding storage"));
    }
    Ok(encoded)
}

/// Materialize an emitted template without parsing or admitting source again.
/// The caller caches this immutable result; captures are bound per invocation.
/// # Errors
/// Rejects malformed encoding, unknown versions, invalid scalar values and IR.
pub fn load(encoded: &str, limits: CompileLimits) -> Result<TypedModule, SemanticError> {
    if encoded.len() > limits.storage_bytes {
        return Err(SemanticError::budget("template encoding storage"));
    }
    let template: Template = serde_json::from_str(encoded)
        .map_err(|error| SemanticError::invalid(format!("template decoding: {error}")))?;
    if template.version != 1 {
        return Err(SemanticError::invalid(
            "unsupported frontend template version",
        ));
    }
    // Verification creates a new publication identity. The serialized snapshot
    // does not grant a capability, and no VerifiedProgram is deserializable.
    let program = template.program.verify(limits)?;
    let module = TypedModule {
        syntax: template.syntax,
        program,
    };
    if module.retained_bytes()? > limits.storage_bytes {
        return Err(SemanticError::budget("template retained storage"));
    }
    Ok(module)
}
