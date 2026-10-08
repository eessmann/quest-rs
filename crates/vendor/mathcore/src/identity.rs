//! Backend-neutral scoped identities; owners explicitly separate symbol namespaces.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Owner(u64);
impl Owner {
	#[must_use]
	pub const fn new(id: u64) -> Self {
		Self(id)
	}
	#[must_use]
	pub const fn id(self) -> u64 {
		self.0
	}
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Symbol {
	owner: Owner,
	index: u64,
}
impl Symbol {
	#[must_use]
	pub const fn new(owner: Owner, index: u64) -> Self {
		Self { owner, index }
	}
	#[must_use]
	pub const fn owner(self) -> Owner {
		self.owner
	}
	#[must_use]
	pub const fn index(self) -> u64 {
		self.index
	}
}
