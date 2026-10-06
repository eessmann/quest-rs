//! Fixed-size private-lane exchange windows. No rank buffers another rank's source.
use super::{ProducerLimits, ProducerStatistics};
use crate::{Error, Result, error::BackendResult};
use quest_sys::mpi::MpiCollectiveLane;

pub(super) const WORDS: usize = 8;
const BYTES: usize = WORDS * size_of::<u64>();
pub(super) struct Packet {
	pub destination: usize,
	pub words: [u64; WORDS],
}
pub(super) struct Router<'a> {
	pub lane: MpiCollectiveLane<'a>,
	pub rank: usize,
	pub parts: usize,
	pub limits: ProducerLimits,
	pub statistics: ProducerStatistics,
	retained: usize,
}
pub(super) fn number(n: usize) -> Result<u64> {
	u64::try_from(n).map_err(|_| Error::Overflow)
}
pub(super) fn index(n: u64) -> Result<usize> {
	usize::try_from(n).map_err(|_| Error::Overflow)
}
pub(super) fn numerical<T>(result: quest_numerics::Result<T>) -> Result<T> {
	result.map_err(|_| Error::Value("streamed sparse input or resource admission rejected"))
}
impl<'a> Router<'a> {
	pub fn new(
		lane: MpiCollectiveLane<'a>,
		rank: usize,
		parts: usize,
		limits: ProducerLimits,
	) -> Result<Self> {
		let retained = limits
			.stream
			.max_bytes
			.checked_add(quest_qsvt::RECORD_FINGERPRINT_SCRATCH_BYTES)
			.ok_or(Error::Overflow)?;
		if limits.batch_entries == 0
			|| i32::try_from(
				limits
					.batch_entries
					.checked_mul(BYTES)
					.ok_or(Error::Overflow)?,
			)
			.is_err()
			|| retained > limits.max_bytes
		{
			return Err(Error::Value("producer transport/storage admission"));
		}
		Ok(Self {
			lane,
			rank,
			parts,
			limits,
			statistics: ProducerStatistics {
				peak_managed_bytes: retained,
				..ProducerStatistics::default()
			},
			retained,
		})
	}
	pub fn agree<T>(&mut self, result: Result<T>) -> Result<T> {
		if !self
			.lane
			.all_agree(result.is_ok())
			.context("agreeing sparse producer admission")?
		{
			return Err(result
				.err()
				.unwrap_or(Error::Value("another producer rank rejected admission")));
		}
		result
	}
	pub fn all(&mut self, value: bool) -> Result<bool> {
		self.lane
			.all_agree(value)
			.context("coordinating sparse producer")
	}
	pub fn work(&mut self, count: usize) -> Result<()> {
		self.statistics.work = self
			.statistics
			.work
			.checked_add(count)
			.ok_or(Error::Overflow)?;
		if self.statistics.work > self.limits.max_work {
			return Err(Error::Value("sparse producer work budget"));
		}
		Ok(())
	}
	pub fn push<T>(&mut self, values: &mut Vec<T>, value: T, limit: usize) -> Result<()> {
		let length = values.len().checked_add(1).ok_or(Error::Overflow)?;
		if length > limit {
			return Err(Error::Value("sparse producer local record limit"));
		}
		if length > values.capacity() {
			let capacity = values
				.capacity()
				.checked_mul(2)
				.unwrap_or(limit)
				.max(length)
				.min(limit);
			// A fresh allocation makes old/new overlap explicit, rather than assuming
			// that an allocator can extend the original allocation in place.
			self.admit_extra(
				capacity
					.checked_mul(size_of::<T>())
					.ok_or(Error::Overflow)?,
			)?;
			let mut grown = Vec::new();
			grown
				.try_reserve_exact(capacity)
				.map_err(|_| Error::Allocation)?;
			self.retain(vector_bytes(&grown)?)?;
			grown.append(values);
			let previous = std::mem::replace(values, grown);
			self.drop_vector(previous)?;
		}
		values.push(value);
		self.work(1)
	}
	fn admit_extra(&self, bytes: usize) -> Result<usize> {
		let requested = self.retained.checked_add(bytes).ok_or(Error::Overflow)?;
		if requested > self.limits.max_bytes {
			return Err(Error::Budget {
				requested,
				available: self.limits.max_bytes,
			});
		}
		Ok(requested)
	}
	fn retain(&mut self, bytes: usize) -> Result<()> {
		self.retained = self.admit_extra(bytes)?;
		self.statistics.peak_managed_bytes = self.statistics.peak_managed_bytes.max(self.retained);
		Ok(())
	}
	pub fn release(&mut self, bytes: usize) -> Result<()> {
		self.retained = self.retained.checked_sub(bytes).ok_or(Error::Overflow)?;
		Ok(())
	}
	/// Release only after the allocation has actually been destroyed. Nested
	/// allocations, such as endpoint color lists, must be released separately.
	pub fn drop_vector<T>(&mut self, values: Vec<T>) -> Result<()> {
		let bytes = vector_bytes(&values)?;
		drop(values);
		self.release(bytes)
	}
	// Only scalar metadata is reduced. No coefficient collection is used.
	pub fn reduce(&mut self, local: u64, merge: impl Fn(u64, u64) -> Result<u64>) -> Result<u64> {
		let charged = self.work(self.parts);
		self.agree(charged)?;
		let mut result = 0;
		for root in 0..self.parts {
			let mut bytes = if root == self.rank {
				local.to_le_bytes()
			} else {
				[0; 8]
			};
			self.lane
				.broadcast_bytes(
					i32::try_from(root).map_err(|_| Error::Overflow)?,
					&mut bytes,
				)
				.context("reducing producer metadata")?;
			result = merge(result, u64::from_le_bytes(bytes))?;
		}
		Ok(result)
	}
	pub fn sum(&mut self, local: usize) -> Result<usize> {
		index(self.reduce(number(local)?, |a, b| {
			a.checked_add(b).ok_or(Error::Overflow)
		})?)
	}
	pub fn maximum(&mut self, local: usize) -> Result<usize> {
		index(self.reduce(number(local)?, |a, b| Ok(a.max(b)))?)
	}
	pub fn digest(&mut self, local: u64) -> Result<u64> {
		self.reduce(local, |a, b| Ok(a.wrapping_add(b)))
	}
	pub fn beta(&mut self, local: f64) -> Result<f64> {
		Ok(f64::from_bits(self.reduce(local.to_bits(), |a, b| {
			Ok(f64::from_bits(a).max(f64::from_bits(b)).to_bits())
		})?))
	}
	/// Two bounded send-receive operations per peer: requests followed by replies.
	/// Handler failures are agreed only after draining every peer in this window.
	pub fn exchange(
		&mut self,
		input: impl IntoIterator<Item = Result<Packet>>,
		mut handle: impl FnMut(&mut Self, [u64; WORDS]) -> Result<[u64; WORDS]>,
		mut reply: impl FnMut(&mut Self, [u64; WORDS]) -> Result<()>,
	) -> Result<()> {
		let mut input = input.into_iter();
		let requested = Workspace::requested_bytes(self.limits.batch_entries)?;
		let admitted = self.admit_extra(requested);
		self.agree(admitted)?;
		let allocated = Workspace::new(self.limits.batch_entries);
		let mut workspace = self.agree(allocated)?;
		let actual = workspace.retained_bytes()?;
		let retained = self.retain(actual);
		self.agree(retained)?;
		let result = (|| {
			loop {
				workspace.batch.clear();
				let filled = (|| {
					for _ in 0..self.limits.batch_entries {
						let Some(packet) = input.next() else { break };
						let packet = packet?;
						if packet.destination >= self.parts {
							return Err(Error::Value("producer packet destination"));
						}
						workspace.batch.push(packet);
					}
					Ok(())
				})();
				self.agree(filled)?;
				if self.all(workspace.batch.is_empty())? {
					return Ok(());
				}
				let mut failure = None;
				for offset in 0..self.parts {
					self.window(
						&mut workspace,
						offset,
						&mut handle,
						&mut reply,
						&mut failure,
					)?;
				}
				self.agree(failure.map_or(Ok(()), Err))?;
			}
		})();
		drop(workspace);
		self.release(actual)?;
		result
	}
	fn window(
		&mut self,
		workspace: &mut Workspace,
		offset: usize,
		handle: &mut impl FnMut(&mut Self, [u64; WORDS]) -> Result<[u64; WORDS]>,
		reply: &mut impl FnMut(&mut Self, [u64; WORDS]) -> Result<()>,
		failure: &mut Option<Error>,
	) -> Result<()> {
		let Workspace {
			batch,
			send,
			receive,
			response,
		} = workspace;
		let peer = self.rank ^ offset;
		let mut length = 0_usize;
		for (packet, bytes) in batch
			.iter()
			.filter(|p| p.destination == peer)
			.zip(send.as_chunks_mut::<BYTES>().0)
		{
			encode(bytes, packet.words);
			length = length.checked_add(BYTES).ok_or(Error::Overflow)?;
		}
		let peer = i32::try_from(peer).map_err(|_| Error::Overflow)?;
		let incoming = self.incoming_length(length, peer, offset, receive.len())?;
		self.charge_window(offset, length, incoming)?;
		let received = if offset == 0 {
			receive
				.get_mut(..length)
				.ok_or(Error::Overflow)?
				.copy_from_slice(send.get(..length).ok_or(Error::Overflow)?);
			length
		} else {
			self.lane
				.send_receive_bytes(
					send.get(..length).ok_or(Error::Overflow)?,
					peer,
					30100,
					receive,
				)
				.unwrap_or_else(|_| quest_sys::mpi::abort_job())
		};
		if received != incoming {
			quest_sys::mpi::abort_job();
		}
		for (packet, out) in receive
			.get(..received)
			.ok_or(Error::Overflow)?
			.as_chunks::<BYTES>()
			.0
			.iter()
			.zip(response.as_chunks_mut::<BYTES>().0)
		{
			let answer = match handle(self, decode(packet)) {
				Ok(value) => value,
				Err(error) => {
					failure.get_or_insert(error);
					[0; WORDS]
				}
			};
			encode(out, answer);
		}
		let answered = if offset == 0 {
			send.get_mut(..received)
				.ok_or(Error::Overflow)?
				.copy_from_slice(response.get(..received).ok_or(Error::Overflow)?);
			received
		} else {
			self.lane
				.send_receive_bytes(
					response.get(..received).ok_or(Error::Overflow)?,
					peer,
					30101,
					send,
				)
				.unwrap_or_else(|_| quest_sys::mpi::abort_job())
		};
		if answered != length {
			quest_sys::mpi::abort_job();
		}
		for packet in send
			.get(..answered)
			.ok_or(Error::Overflow)?
			.as_chunks::<BYTES>()
			.0
		{
			if failure.is_none()
				&& let Err(error) = reply(self, decode(packet))
			{
				*failure = Some(error);
			}
		}
		Ok(())
	}
	fn charge_window(&mut self, offset: usize, length: usize, incoming: usize) -> Result<()> {
		let charged = (|| {
			let reserved = if offset == 0 {
				0
			} else {
				length.checked_add(incoming).ok_or(Error::Overflow)?
			};
			let requested = self
				.statistics
				.sent_bytes
				.checked_add(reserved)
				.ok_or(Error::Overflow)?;
			if requested > self.limits.max_communication_bytes {
				return Err(Error::Value("sparse producer wire budget"));
			}
			self.work(self.limits.batch_entries)?;
			self.statistics.sent_bytes = requested;
			Ok(())
		})();
		self.agree(charged)
	}
	fn incoming_length(
		&mut self,
		length: usize,
		peer: i32,
		offset: usize,
		bound: usize,
	) -> Result<usize> {
		if offset == 0 {
			return Ok(length);
		}
		let mut bytes = [0; 8];
		let received = self
			.lane
			.send_receive_bytes(&number(length)?.to_le_bytes(), peer, 30102, &mut bytes)
			.unwrap_or_else(|_| quest_sys::mpi::abort_job());
		if received != bytes.len() {
			quest_sys::mpi::abort_job();
		}
		let length = index(u64::from_le_bytes(bytes))?;
		if length > bound || length.checked_rem(BYTES) != Some(0) {
			quest_sys::mpi::abort_job();
		}
		Ok(length)
	}
}
struct Workspace {
	batch: Vec<Packet>,
	send: Vec<u8>,
	receive: Vec<u8>,
	response: Vec<u8>,
}
impl Workspace {
	fn requested_bytes(count: usize) -> Result<usize> {
		count
			.checked_mul(
				size_of::<Packet>()
					.checked_add(BYTES.checked_mul(3).ok_or(Error::Overflow)?)
					.ok_or(Error::Overflow)?,
			)
			.and_then(|n| n.checked_add(size_of::<Self>()))
			.ok_or(Error::Overflow)
	}
	fn retained_bytes(&self) -> Result<usize> {
		[
			vector_bytes(&self.batch)?,
			vector_bytes(&self.send)?,
			vector_bytes(&self.receive)?,
			vector_bytes(&self.response)?,
			size_of::<Self>(),
		]
		.into_iter()
		.try_fold(0usize, |sum, bytes| {
			sum.checked_add(bytes).ok_or(Error::Overflow)
		})
	}
	fn new(count: usize) -> Result<Self> {
		let mut batch = Vec::new();
		batch
			.try_reserve_exact(count)
			.map_err(|_| Error::Allocation)?;
		let bytes = count.checked_mul(BYTES).ok_or(Error::Overflow)?;
		Ok(Self {
			batch,
			send: buffer(bytes)?,
			receive: buffer(bytes)?,
			response: buffer(bytes)?,
		})
	}
}
fn vector_bytes<T>(values: &Vec<T>) -> Result<usize> {
	values
		.capacity()
		.checked_mul(size_of::<T>())
		.ok_or(Error::Overflow)
}
fn encode(bytes: &mut [u8; BYTES], words: [u64; WORDS]) {
	for (word, bytes) in words.iter().zip(bytes.as_chunks_mut::<8>().0) {
		bytes.copy_from_slice(&word.to_le_bytes());
	}
}
fn decode(bytes: &[u8; BYTES]) -> [u64; WORDS] {
	let mut words = [0; WORDS];
	for (word, bytes) in words.iter_mut().zip(bytes.as_chunks::<8>().0) {
		*word = u64::from_le_bytes(*bytes);
	}
	words
}
fn buffer(bytes: usize) -> Result<Vec<u8>> {
	let mut result = Vec::new();
	result
		.try_reserve_exact(bytes)
		.map_err(|_| Error::Allocation)?;
	result.resize(bytes, 0);
	Ok(result)
}
