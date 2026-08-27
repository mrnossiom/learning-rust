#![feature(allocator_api)]

use std::{
	alloc::{Allocator, Global, Layout},
	cell::Cell,
	marker::PhantomData,
	ptr::{self, NonNull},
};

struct Idx<'a, T>(u32, PhantomData<&'a T>);

struct Arena<A: Allocator = Global> {
	start: Cell<NonNull<u8>>,
	end: Cell<NonNull<u8>>,
	ptr: Cell<NonNull<u8>>,

	alloc: A,
}

impl Arena {
	pub fn new() -> Self {
		Self::new_in(Global)
	}
}

impl<A: Allocator> Arena<A> {
	pub fn new_in(alloc: A) -> Self {
		Self::try_new_in(alloc).unwrap()
	}

	pub fn try_new_in(alloc: A) -> Option<Self> {
		let layout = Layout::array::<u8>(4).expect("4 bytes is small enough");

		let allocated = alloc.allocate(layout).ok()?;

		let ptr = allocated.cast::<u8>();

		let start = Cell::new(ptr);
		// SAFETY: todo
		let end = Cell::new(unsafe { ptr.add(allocated.len()) });

		Some(Self {
			start,
			ptr: end.clone(),
			end,
			alloc,
		})
	}

	pub fn alloc<T>(&self, val: T) -> Idx<'_, T> {
		self.try_alloc(val).unwrap()
	}

	pub fn try_alloc<T>(&self, val: T) -> Option<Idx<'_, T>> {
		let ptr = self.try_alloc_layout(Layout::new::<T>())?;
		unsafe { ptr::write(ptr.as_ptr().cast(), val) };

		let offset_from_end = self.end.get().addr().get() - ptr.addr().get();
		let idx = Idx(u32::try_from(offset_from_end).ok()?, PhantomData);
		Some(idx)
	}

	pub fn try_alloc_layout(&self, layout: Layout) -> Option<NonNull<u8>> {
		if let Some(ptr) = self.try_alloc_layout_fast(layout) {
			Some(ptr)
		} else {
			self.try_alloc_layout_realloc(layout)
		}
	}

	fn try_alloc_layout_fast(&self, layout: Layout) -> Option<NonNull<u8>> {
		let next_ptr = bump_down_layout(self.ptr.get().as_ptr(), layout)?;

		// not enough space, goto slow realloc path
		if next_ptr < self.start.get().as_ptr() {
			return None;
		}

		debug_assert!(!next_ptr.is_null());
		let next_ptr = unsafe { NonNull::new_unchecked(next_ptr) };
		self.ptr.set(next_ptr);

		Some(next_ptr)
	}

	fn try_alloc_layout_realloc(&self, layout: Layout) -> Option<NonNull<u8>> {
		// TODO: loop in case we want to alloc a large struct

		let end = self.end.get().as_ptr().addr();
		let start = self.start.get().as_ptr().addr();
		let low = bump_down_layout(self.ptr.get().as_ptr(), layout)
			.unwrap()
			.addr();
		let cap_real = end - start;
		let cap_needed = end - low;

		// round cap to closest power of two
		let rounded_cap = 1usize << (usize::BITS.wrapping_sub(cap_real.leading_zeros()));

		let mut next_cap = rounded_cap;
		while next_cap < cap_needed {
			next_cap = next_cap.checked_mul(2)?;
		}

		let next_layout = Layout::array::<u8>(next_cap).unwrap();

		let next_allocated = self.alloc.allocate(next_layout).ok()?;
		let next_ptr = next_allocated.cast::<u8>();
		let next_end = unsafe { next_ptr.add(next_allocated.len()) };

		// SAFETY: ?
		unsafe {
			// TODO: copies uninit memory?

			let len = self.end.get().addr().get() - self.ptr.get().addr().get();

			ptr::copy_nonoverlapping::<u8>(
				self.ptr.get().as_ptr(),
				next_end.sub(len).as_ptr(),
				len,
			);
		}

		// SAFETY: ?
		unsafe {
			let prev_layout = Layout::array::<u8>(cap_real).unwrap();
			self.alloc.deallocate(self.start.get(), prev_layout);
		}

		self.start.set(next_ptr);
		self.ptr.set(next_end);
		self.end.set(next_end);

		self.try_alloc_layout_fast(layout)
	}

	pub fn get<'a, T>(&'a self, idx: Idx<'a, T>) -> &'a T {
		self.try_get(idx).unwrap()
	}

	pub fn try_get<'a, T>(&'a self, idx: Idx<'a, T>) -> Option<&'a T> {
		let offset = usize::try_from(idx.0).ok()?;
		let start = self.end.get();
		let current = unsafe { start.sub(offset).cast().as_ref() };
		Some(current)
	}
}

impl<A: Allocator> Arena<A> {
	pub fn as_slice(&self) -> &[u8] {
		unsafe {
			let ptr = self.ptr.get().as_ptr();
			let end = self.end.get().as_ptr();
			std::slice::from_raw_parts(ptr, end.addr() - ptr.addr())
		}
	}
}

impl<A: Allocator> Drop for Arena<A> {
	fn drop(&mut self) {
		let layout =
			Layout::array::<u8>(self.end.get().addr().get() - self.start.get().addr().get())
				.unwrap();
		unsafe {
			self.alloc.deallocate(self.start.get(), layout);
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
	use super::Arena;

	#[test]
	#[ignore]
	fn full_circle() {
		let arena = Arena::new();

		for _ in 0..1000 {
			_ = arena.alloc(4);
		}

		let h = arena.alloc(3);
		let v = arena.get(h);

		assert_eq!(*v, 3);
	}

	#[test]
	fn large_item() {
		#[repr(align(256))]
		struct Foo(u32);

		let arena = Arena::new();

		let h = arena.alloc(Foo(1));
		let v = arena.get(h);

		let raw = arena.as_slice().to_vec();
		dbg!(raw);

		assert_eq!(v.0, 1);
	}
}
