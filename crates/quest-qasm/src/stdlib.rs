use crate::{IncludeResolver, ResolveError};
use quest_language::{SourceId, SourceSnapshot};
/// Unmodified source from the official 3.1 release commit.
pub const UPSTREAM_STANDARD_GATES: &str = include_str!("stdlib/stdgates.upstream.inc");
/// Corrected specification-exact standard library, with original source retained.
pub const STANDARD_GATES: &str = include_str!("stdlib/stdgates.inc");
pub const UPSTREAM_COMMIT: &str = "c717508162a0eac892fa32134716fe77a284e835";
/// An explicit resolver for the bundled library; all other requests fail.
#[derive(Debug, Clone)]
pub struct StandardLibrary {
	source: SourceSnapshot,
}
impl StandardLibrary {
	/// Allocate a source identity that is distinct from the caller's source IDs.
	#[must_use]
	pub fn new(id: SourceId) -> Self {
		Self {
			source: SourceSnapshot::new(id, "stdgates.inc", STANDARD_GATES),
		}
	}
}
impl IncludeResolver for StandardLibrary {
	fn resolve(
		&mut self,
		_: &SourceSnapshot,
		path: &str,
	) -> std::result::Result<SourceSnapshot, ResolveError> {
		if path == "stdgates.inc" {
			Ok(self.source.clone())
		} else {
			Err(ResolveError::new(
				"only the pinned stdgates.inc is available",
			))
		}
	}
}
