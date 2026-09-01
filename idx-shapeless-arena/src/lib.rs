#![feature(allocator_api)]
#![warn(clippy::undocumented_unsafe_blocks)]

use core::fmt;
use std::{
	alloc::{Allocator, Global, Layout},
	marker::PhantomData,
	ptr::{self, NonNull},
};
use zerocopy::{FromBytes, Immutable, IntoBytes, KnownLayout, TryFromBytes};

#[derive(Debug)]
pub struct ArenaErr;

#[derive(IntoBytes, FromBytes, KnownLayout, Immutable)]
#[repr(transparent)]
pub struct Idx<T>(u32, PhantomData<fn() -> T>);

impl<T> Copy for Idx<T> {}
impl<T> Clone for Idx<T> {
	fn clone(&self) -> Self {
		*self
	}
}

impl<T> fmt::Debug for Idx<T> {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		write!(f, "Idx({:08x})", self.0)
	}
}

/// A heterogenous arena that hands lifetime-free 4-bytes indices whose backing memory can be (de)serialized.
pub struct Arena<const MIN_ALIGN: usize, const MAX_ALIGN: usize, A: Allocator = Global> {
	/// Start of the allocated memory slice.
	///
	/// It is only used used during reallocation phase
	start: NonNull<u8>,
	/// End of the last layout allocated
	end: NonNull<u8>,
	/// End of the allocated memory region. Used to compute offsets with [`Idx`]
	ptr: NonNull<u8>,

	/// Allocator used to allocate the memory
	alloc: A,
}

impl<const MIN_ALIGN: usize, const MAX_ALIGN: usize> Default for Arena<MIN_ALIGN, MAX_ALIGN> {
	fn default() -> Self {
		Self::new_in(Global)
	}
}

impl<const MIN_ALIGN: usize, const MAX_ALIGN: usize, A: Allocator> Arena<MIN_ALIGN, MAX_ALIGN, A> {
	pub fn new_in(alloc: A) -> Self {
		Self::try_new_in(alloc).unwrap()
	}

	pub fn try_new_in(alloc: A) -> Option<Self> {
		assert!(
			MIN_ALIGN.is_power_of_two(),
			"MIN_ALIGN is not a power of two"
		);
		assert!(
			MAX_ALIGN.is_power_of_two(),
			"MIN_ALIGN is not a power of two"
		);
		assert!(
			MIN_ALIGN <= MAX_ALIGN,
			"MIN_ALIGN cannot be larger than MAX_ALIGN"
		);

		let layout = Layout::array::<u8>(MIN_ALIGN * 4)
			.unwrap()
			.align_to(MAX_ALIGN)
			.unwrap();

		// if a single alignment is possible, we don't need to init padding bytes, we can let the whole memory be uninit
		let allocated = if MIN_ALIGN == MAX_ALIGN {
			alloc.allocate(layout)
		} else {
			alloc.allocate_zeroed(layout)
		}
		.ok()?;

		let start = allocated.cast::<u8>();
		// SAFETY: we added the length of the allocation which is in bounds
		let end = unsafe { start.add(allocated.len()) };

		Some(Self {
			start,
			ptr: end,
			end,
			alloc,
		})
	}

	#[inline(always)]
	pub fn alloc<T: KnownLayout + Immutable + IntoBytes>(&mut self, val: T) -> Idx<T> {
		self.try_alloc(val).unwrap()
	}

	pub fn try_alloc<T: KnownLayout + Immutable + IntoBytes>(
		&mut self,
		val: T,
	) -> Result<Idx<T>, ArenaErr> {
		assert!(
			!std::mem::needs_drop::<T>(),
			"value will not be dropped, maybe use ManuallyDrop?"
		);

		// SAFETY: we immediately write to the given pointer
		let ptr = unsafe { self.try_alloc_layout(Layout::new::<T>())? };
		// SAFETY: the pointer is allocated for the exact layout of T
		unsafe { ptr::write(ptr.as_ptr().cast(), val) };

		let backward_offset = self.end.addr().get() - ptr.addr().get();
		let backward_offset = u32::try_from(backward_offset).map_err(|_| ArenaErr)?;
		let idx = Idx(backward_offset, PhantomData);
		Ok(idx)
	}

	/// # Safety
	///
	/// The returned pointer needs to be written to before any serialization function is used.
	pub unsafe fn try_alloc_layout(&mut self, layout: Layout) -> Result<NonNull<u8>, ArenaErr> {
		assert!(layout.align() >= MIN_ALIGN, "alignment is too small");
		assert!(layout.align() <= MAX_ALIGN, "alignment is too large");

		if let Some(ptr) = self.try_alloc_layout_fast(layout) {
			Ok(ptr)
		} else {
			self.try_alloc_layout_realloc(layout)
		}
	}

	fn try_alloc_layout_fast(&mut self, layout: Layout) -> Option<NonNull<u8>> {
		let next_ptr = bump_down_layout(self.ptr.as_ptr(), layout)?;

		// not enough space, goto slow realloc path
		if next_ptr < self.start.as_ptr() {
			return None;
		}

		let next_ptr = unsafe { NonNull::new_unchecked(next_ptr) };
		self.ptr = next_ptr;

		Some(next_ptr)
	}

	fn try_alloc_layout_realloc(&mut self, layout: Layout) -> Result<NonNull<u8>, ArenaErr> {
		let end = self.end.as_ptr().addr();
		let start = self.start.as_ptr().addr();
		let low = bump_down_layout(self.ptr.as_ptr(), layout).unwrap().addr();

		let cap_real = end - start;
		let cap_needed = end - low;

		let mut next_cap = cap_real.next_power_of_two();
		while next_cap < cap_needed {
			next_cap = next_cap.checked_mul(2).ok_or(ArenaErr)?;
		}

		let next_layout = Layout::array::<u8>(next_cap)
			.unwrap()
			.align_to(MAX_ALIGN)
			.unwrap();

		let next_allocated = self
			.alloc
			.allocate_zeroed(next_layout)
			.map_err(|_| ArenaErr)?;
		let next_start = next_allocated.cast::<u8>();
		// SAFETY: we add the length of the allocation which is in bounds
		let next_end = unsafe { next_start.add(next_allocated.len()) };

		let len = self.end.addr().get() - self.ptr.addr().get();
		let next_current = unsafe { next_end.sub(len) };

		// SAFETY: we risk to copy uninit bytes if not careful here
		//
		// - all types that are allocated within the arena implement [`zerocopy::FromBytes`]
		//
		// - for padding bytes between allocations
		//   + if `MAX_ALIGN == MIN_ALIGN`: there is none
		//   + else: we zero the memory when allocating such that there is no uninit byte
		unsafe {
			ptr::copy_nonoverlapping(self.ptr.as_ptr(), next_current.as_ptr(), len);
		}

		// SAFETY:
		// - we use the given start of the allocation
		// - right layout size with `end - start` pointer arithmetic
		// - right layout align which is always `MAX_ALIGN`
		unsafe {
			let prev_layout = Layout::array::<u8>(cap_real)
				.unwrap()
				.align_to(MAX_ALIGN)
				.unwrap();
			self.alloc.deallocate(self.start, prev_layout);
		}

		self.start = next_start;
		self.ptr = next_current;
		self.end = next_end;

		self.try_alloc_layout_fast(layout).ok_or(ArenaErr)
	}

	#[inline(always)]
	pub fn get<T: KnownLayout + Immutable + FromBytes>(&self, idx: Idx<T>) -> &T {
		self.try_get(idx).unwrap()
	}

	pub fn try_get<T: KnownLayout + Immutable + FromBytes>(
		&self,
		idx: Idx<T>,
	) -> Result<&T, ArenaErr> {
		let ptr = self.try_get_ptr(idx)?;
		let source = unsafe { std::slice::from_raw_parts(ptr.as_ptr(), Layout::new::<T>().size()) };
		<T as FromBytes>::ref_from_bytes(source).map_err(|_| ArenaErr)
	}

	#[inline(always)]
	pub fn get_partial<T: KnownLayout + Immutable + TryFromBytes>(&self, idx: Idx<T>) -> &T {
		self.try_get_partial(idx).unwrap()
	}

	pub fn try_get_partial<T: KnownLayout + Immutable + TryFromBytes>(
		&self,
		idx: Idx<T>,
	) -> Result<&T, ArenaErr> {
		let ptr = self.try_get_ptr(idx)?;
		let source = unsafe { std::slice::from_raw_parts(ptr.as_ptr(), Layout::new::<T>().size()) };
		<T as TryFromBytes>::try_ref_from_bytes(source).map_err(|_| ArenaErr)
	}

	#[inline(always)]
	pub fn try_get_ptr<T>(&self, idx: Idx<T>) -> Result<NonNull<u8>, ArenaErr> {
		let offset = usize::try_from(idx.0).map_err(|_| ArenaErr)?;
		let ptr = unsafe { self.end.sub(offset).cast() };
		Ok(ptr)
	}
}

impl<const MIN_ALIGN: usize, const MAX_ALIGN: usize, A: Allocator> Arena<MIN_ALIGN, MAX_ALIGN, A> {
	pub fn as_slice(&self) -> &[u8] {
		// SAFETY: this is (NOT YET) valid
		// - values written in the range implement the trait `zerocopy::IntoBytes`
		// - (WIP) when rounding down the pointer for alignment, we can create holes of uninit data
		unsafe {
			let len = self.end.as_ptr().addr() - self.ptr.addr().get();
			std::slice::from_raw_parts(self.ptr.as_ptr(), len)
		}
	}
}

impl<const MIN_ALIGN: usize, const MAX_ALIGN: usize, A: Allocator> Drop
	for Arena<MIN_ALIGN, MAX_ALIGN, A>
{
	fn drop(&mut self) {
		let n = self.end.addr().get() - self.start.addr().get();
		let layout = Layout::array::<u8>(n).unwrap().align_to(MAX_ALIGN).unwrap();
		// SAFETY:
		// - we use the ptr to the start of the alloc
		// - layout is correct
		unsafe {
			self.alloc.deallocate(self.start, layout);
		}
	}
}

/// Bumps down a pointer by layout's size and flooring as needed by the alignment
fn bump_down_layout(ptr: *mut u8, layout: Layout) -> Option<*mut u8> {
	let raw_next_ptr = ptr.addr().checked_sub(layout.size())?;
	let aligned_next_ptr = raw_next_ptr & !(layout.align() - 1);
	Some(ptr.with_addr(aligned_next_ptr))
}

#[cfg(test)]
mod tests {
	use std::assert_matches;

	use zerocopy::{Immutable, IntoBytes, KnownLayout, TryFromBytes};

	use super::{Arena, Idx};

	fn print_memory_formatted(mem: &[u8]) {
		let (chunks, rest) = mem.as_chunks::<8>();
		for [b1, b2, b3, b4, b5, b6, b7, b8] in chunks {
			println!("{b1:02x} {b2:02x} {b3:02x} {b4:02x} {b5:02x} {b6:02x} {b7:02x} {b8:02x} ")
		}
		for b in rest {
			print!("{b:02x} ")
		}
		println!()
	}

	#[test]
	fn alloc_small_tree_with_artificial_padding() {
		#[derive(Debug, KnownLayout, Immutable, IntoBytes, TryFromBytes)]
		#[repr(u32)]
		enum Pad32 {
			Null = 0x0,
		}

		#[derive(Debug, KnownLayout, Immutable, IntoBytes, TryFromBytes)]
		#[repr(u32)]
		enum Foo {
			Def { id: u32, _pad: Pad32 },
			BinOp { lhs: Idx<Foo>, rhs: Idx<Foo> },
		}

		let mut arena = Arena::<1, 4>::default();

		let def1 = arena.alloc(Foo::Def {
			id: 1,
			_pad: Pad32::Null,
		});
		let def2 = arena.alloc(Foo::Def {
			id: 2,
			_pad: Pad32::Null,
		});
		let _binop = arena.alloc(Foo::BinOp {
			lhs: def1,
			rhs: def2,
		});

		let v1 = arena.get_partial(def1);
		assert_matches!(v1, Foo::Def { id: 1, .. });
		let v2 = arena.get_partial(def2);
		assert_matches!(v2, Foo::Def { id: 2, .. });
	}

	#[test]
	fn realloc_heterogenous() {
		let mut arena = Arena::<1, 4>::default();

		let r1 = 0..100u8;
		let r2 = 0..100u16;
		let r3 = 0..100u32;

		for (e1, (e2, e3)) in r1.zip(r2.zip(r3)) {
			let h3 = arena.alloc(e3);
			let h2 = arena.alloc(e2);
			let h1 = arena.alloc(e1);

			let v1 = arena.get(h1);
			assert_eq!(e1, *v1);
			let v2 = arena.get(h2);
			assert_eq!(e2, *v2);
			let v3 = arena.get(h3);
			assert_eq!(e3, *v3);
		}

		// print_memory_formatted(arena.as_slice());
	}

	#[test]
	#[should_panic]
	fn alloc_over_max_align_fail() {
		let mut arena = Arena::<1, 1>::default();

		let _h = arena.alloc(0u16);
	}

	#[test]
	#[should_panic]
	fn alloc_needs_drop_fail() {
		let mut arena = Arena::<1, 1>::default();

		#[derive(Debug, KnownLayout, Immutable, IntoBytes, TryFromBytes)]
		struct Foo;

		impl Drop for Foo {
			fn drop(&mut self) {
				todo!()
			}
		}

		let _h = arena.alloc(Foo);
	}

	#[test]
	fn ser_full_circle() {
		let mut arena = Arena::<2, 4>::default();

		let _h = arena.alloc(0u16);

		let mem = arena.as_slice();

		let new_arena = Arena::<2, 4>::default();
	}
}
