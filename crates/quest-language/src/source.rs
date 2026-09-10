//! Immutable, caller-identified source snapshots and checked byte spans.
use std::{collections::BTreeMap, ops::Range, sync::Arc};

/// Identity within one compilation. Allocate a fresh ID for every snapshot,
/// including different revisions that have the same display filename.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct SourceId(u64);
impl SourceId {
    #[must_use]
    pub const fn new(value: u64) -> Self {
        Self(value)
    }
    #[must_use]
    pub const fn value(self) -> u64 {
        self.0
    }
}

/// Owned source text; neither the filename nor the contents are resolved from disk.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct SourceSnapshot {
    id: SourceId,
    name: Arc<str>,
    text: Arc<str>,
}
impl SourceSnapshot {
    #[must_use]
    pub fn new(id: SourceId, name: impl Into<String>, text: impl Into<String>) -> Self {
        Self {
            id,
            name: Arc::from(name.into()),
            text: Arc::from(text.into()),
        }
    }
    #[must_use]
    pub const fn id(&self) -> SourceId {
        self.id
    }
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }
    #[must_use]
    pub fn text(&self) -> &str {
        &self.text
    }
    /// Check a half-open byte range, including UTF-8 boundaries.
    ///
    /// # Errors
    /// Rejects reversed, out-of-bounds, or non-character-boundary ranges.
    pub fn span(&self, range: Range<usize>) -> Result<SourceSpan, SourceError> {
        if range.start > range.end {
            return Err(SourceError::ReversedRange);
        }
        if range.end > self.text.len() {
            return Err(SourceError::OutOfBounds);
        }
        if !self.text.is_char_boundary(range.start) || !self.text.is_char_boundary(range.end) {
            return Err(SourceError::NotCharBoundary);
        }
        Ok(SourceSpan {
            source: self.id,
            start: range.start,
            end: range.end,
        })
    }
    /// Resolve and revalidate a span, including deserialized spans.
    ///
    /// # Errors
    /// Rejects foreign source identities or invalid ranges.
    pub fn slice(&self, span: SourceSpan) -> Result<&str, SourceError> {
        if self.id != span.source {
            return Err(SourceError::WrongSource);
        }
        self.span(span.range())?;
        self.text.get(span.range()).ok_or(SourceError::OutOfBounds)
    }
}

/// A span constructed against a snapshot. Consumers revalidate deserialized
/// ranges against their owned snapshot before accessing text.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(try_from = "SerializedSpan"))]
pub struct SourceSpan {
    source: SourceId,
    start: usize,
    end: usize,
}
impl SourceSpan {
    /// Admit compiler-provided coordinates when source text is unavailable.
    ///
    /// Consumers with a snapshot still revalidate UTF-8 and bounds. A frontend
    /// must retain a display location for sources without a text snapshot.
    ///
    /// # Errors
    /// Rejects a reversed half-open range.
    pub const fn location(source: SourceId, range: Range<usize>) -> Result<Self, SourceError> {
        if range.start > range.end {
            return Err(SourceError::ReversedRange);
        }
        Ok(Self {
            source,
            start: range.start,
            end: range.end,
        })
    }

    #[must_use]
    pub const fn source(self) -> SourceId {
        self.source
    }
    #[must_use]
    pub const fn range(self) -> Range<usize> {
        self.start..self.end
    }
}
#[cfg(feature = "serde")]
#[derive(serde::Deserialize)]
struct SerializedSpan {
    source: SourceId,
    start: usize,
    end: usize,
}
#[cfg(feature = "serde")]
impl TryFrom<SerializedSpan> for SourceSpan {
    type Error = SourceError;
    fn try_from(value: SerializedSpan) -> Result<Self, Self::Error> {
        if value.start > value.end {
            return Err(SourceError::ReversedRange);
        }
        Ok(Self {
            source: value.source,
            start: value.start,
            end: value.end,
        })
    }
}

/// Immutable entries indexed by identity. Reusing an identity is rejected.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(
    feature = "serde",
    serde(try_from = "Vec<SourceSnapshot>", into = "Vec<SourceSnapshot>")
)]
pub struct SourceMap {
    sources: BTreeMap<SourceId, SourceSnapshot>,
}
impl SourceMap {
    /// Add an owned immutable snapshot.
    ///
    /// # Errors
    /// Returns `DuplicateIdentity` without replacing an existing entry.
    pub fn insert(&mut self, source: SourceSnapshot) -> Result<(), SourceError> {
        match self.sources.entry(source.id()) {
            std::collections::btree_map::Entry::Vacant(entry) => {
                entry.insert(source);
                Ok(())
            }
            std::collections::btree_map::Entry::Occupied(_) => Err(SourceError::DuplicateIdentity),
        }
    }
    #[must_use]
    pub fn get(&self, id: SourceId) -> Option<&SourceSnapshot> {
        self.sources.get(&id)
    }
    pub fn iter(&self) -> impl Iterator<Item = &SourceSnapshot> {
        self.sources.values()
    }
    /// Resolve a span against the source collection.
    ///
    /// # Errors
    /// Rejects unknown sources and invalid ranges.
    pub fn slice(&self, span: SourceSpan) -> Result<&str, SourceError> {
        self.get(span.source())
            .ok_or(SourceError::UnknownSource)?
            .slice(span)
    }
}
impl TryFrom<Vec<SourceSnapshot>> for SourceMap {
    type Error = SourceError;
    fn try_from(sources: Vec<SourceSnapshot>) -> Result<Self, Self::Error> {
        let mut map = Self::default();
        for source in sources {
            map.insert(source)?;
        }
        Ok(map)
    }
}
impl From<SourceMap> for Vec<SourceSnapshot> {
    fn from(map: SourceMap) -> Self {
        map.sources.into_values().collect()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum SourceError {
    #[error("source range is reversed")]
    ReversedRange,
    #[error("source range exceeds the snapshot")]
    OutOfBounds,
    #[error("source range splits a UTF-8 character")]
    NotCharBoundary,
    #[error("span belongs to a different source")]
    WrongSource,
    #[error("source identity is absent from the source map")]
    UnknownSource,
    #[error("source identity already exists")]
    DuplicateIdentity,
}
