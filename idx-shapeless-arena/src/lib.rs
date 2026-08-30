#![feature(allocator_api)]

use core::fmt;
use std::{
	alloc::{Allocator, Global, Layout},
	marker::PhantomData,
	ptr::{self, NonNull},
};
use zerocopy::{FromBytes, Immutable, IntoBytes, KnownLayout};

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
		f.debug_tuple("Idx").field(&self.0).finish()
	}
}

pub struct Arena<const MAX_ALIGN: usize, A: Allocator = Global> {
	start: NonNull<u8>,
	end: NonNull<u8>,
	ptr: NonNull<u8>,

	alloc: A,
}

impl<const MAX_ALIGN: usize> Default for Arena<MAX_ALIGN> {
	fn default() -> Self {
		Self::new_in(Global)
	}
}

impl<const MAX_ALIGN: usize, A: Allocator> Arena<MAX_ALIGN, A> {
	pub fn new_in(alloc: A) -> Self {
		Self::try_new_in(alloc).unwrap()
	}

	pub fn try_new_in(alloc: A) -> Option<Self> {
		let layout = Layout::array::<u8>(4)
			.expect("4 bytes is small enough")
			.align_to(MAX_ALIGN)
			.expect("invalid alignment");

		let allocated = alloc.allocate(layout).ok()?;
		let allocated_ptr = allocated.cast::<u8>();

		let start = allocated_ptr;

		// SAFETY: todo
		let end = unsafe { allocated_ptr.add(allocated.len()) };

		Some(Self {
			start,
			ptr: end,
			end,
			alloc,
		})
	}

	pub fn alloc<T: IntoBytes>(&mut self, val: T) -> Idx<T> {
		self.try_alloc(val).unwrap()
	}

	pub fn try_alloc<T>(&mut self, val: T) -> Option<Idx<T>> {
		let ptr = self.try_alloc_layout(Layout::new::<T>())?;
		unsafe { ptr::write(ptr.as_ptr().cast(), val) };

		let offset_from_end = self.end.addr().get() - ptr.addr().get();
		let idx = Idx(u32::try_from(offset_from_end).ok()?, PhantomData);
		Some(idx)
	}

	pub fn try_alloc_layout(&mut self, layout: Layout) -> Option<NonNull<u8>> {
		assert!(layout.align() <= MAX_ALIGN);

		if let Some(ptr) = self.try_alloc_layout_fast(layout) {
			Some(ptr)
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

		debug_assert!(!next_ptr.is_null());
		let next_ptr = unsafe { NonNull::new_unchecked(next_ptr) };
		self.ptr = next_ptr;

		Some(next_ptr)
	}

	fn try_alloc_layout_realloc(&mut self, layout: Layout) -> Option<NonNull<u8>> {
		// TODO: loop in case we want to alloc a large struct

		let end = self.end.as_ptr().addr();
		let start = self.start.as_ptr().addr();
		let low = bump_down_layout(self.ptr.as_ptr(), layout).unwrap().addr();
		let cap_real = end - start;
		let cap_needed = end - low;

		// round cap to closest power of two
		let rounded_cap = cap_real.next_power_of_two();

		let mut next_cap = rounded_cap;
		while next_cap < cap_needed {
			next_cap = next_cap.checked_mul(2)?;
		}

		let next_layout = Layout::array::<u8>(next_cap)
			.unwrap()
			.align_to(MAX_ALIGN)
			.expect("invalid alignment");

		let next_allocated = self.alloc.allocate(next_layout).ok()?;
		let next_start = next_allocated.cast::<u8>();
		let next_end = unsafe { next_start.add(next_allocated.len()) };

		let len = self.end.addr().get() - self.ptr.addr().get();
		let next_current = unsafe { next_end.sub(len) };

		// SAFETY: ?
		unsafe {
			// TODO: copies uninit memory?

			ptr::copy_nonoverlapping::<u8>(self.ptr.as_ptr(), next_current.as_ptr(), len);
		}

		// SAFETY: ?
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

		self.try_alloc_layout_fast(layout)
	}

	pub fn get<T>(&self, idx: Idx<T>) -> &T {
		self.try_get(idx).unwrap()
	}

	pub fn try_get<T>(&self, idx: Idx<T>) -> Option<&T> {
		let offset = usize::try_from(idx.0).ok()?;
		let current = unsafe { self.end.sub(offset).cast().as_ref() };
		Some(current)
	}
}

impl<const MAX_ALIGN: usize, A: Allocator> Arena<MAX_ALIGN, A> {
	pub fn as_slice(&self) -> &[u8] {
		// SAFETY: this is (NOT YET) valid
		// - values written in the range implement the trait `zerocopy::IntoBytes`
		// - (WIP) when rounding down the pointer for alignment, we can create holes of uninit data
		unsafe {
			let ptr = self.ptr.as_ptr();
			let end = self.end.as_ptr();
			std::slice::from_raw_parts(ptr, end.addr() - ptr.addr())
		}
	}
}

impl<const MAX_ALIGN: usize, A: Allocator> Drop for Arena<MAX_ALIGN, A> {
	fn drop(&mut self) {
		let n = self.end.addr().get() - self.start.addr().get();
		let layout = Layout::array::<u8>(n).unwrap().align_to(MAX_ALIGN).unwrap();
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

	#[test]
	fn alloc_small_tree_with_artificial_padding() {
		#[derive(Debug, IntoBytes, TryFromBytes, KnownLayout, Immutable)]
		#[repr(u32)]
		enum Pad32 {
			Null = 0x0,
		}

		#[derive(Debug, IntoBytes, TryFromBytes, KnownLayout, Immutable)]
		#[repr(u32)]
		enum Foo {
			Def { id: u32, _pad: Pad32 },
			BinOp { lhs: Idx<Foo>, rhs: Idx<Foo> },
		}

		let mut arena = Arena::<{ align_of::<u32>() }>::default();

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

		let v1 = arena.get(def1);
		assert_matches!(v1, Foo::Def { id: 1, .. });
		let v2 = arena.get(def2);
		assert_matches!(v2, Foo::Def { id: 2, .. });
	}

	#[test]
	fn realloc_heterogenous() {
		let mut arena = Arena::<{ align_of::<u32>() }>::default();

		let r1 = 0..100u8;
		let r2 = 0..100u16;
		let r3 = 0..100u32;

		for (e1, (e2, e3)) in r1.zip(r2.zip(r3)) {
			let h1 = arena.alloc(e1);
			let h2 = arena.alloc(e2);
			let h3 = arena.alloc(e3);

			let v1 = arena.get(h1);
			assert_eq!(e1, *v1);
			let v2 = arena.get(h2);
			assert_eq!(e2, *v2);
			let v3 = arena.get(h3);
			assert_eq!(e3, *v3);
		}

		for ele in arena.as_slice().iter() {
			print!("{ele:02x} ")
		}
		println!()
	}

	#[test]
	#[should_panic]
	fn alloc_over_max_align_fail() {
		let mut arena = Arena::<{ align_of::<u8>() }>::default();

		let _h = arena.alloc(0u16);
	}

	#[test]
	fn ser_full_circle() {
		let mut arena = Arena::<{ align_of::<u32>() }>::default();

		let _h = arena.alloc(0u16);
	}
}
