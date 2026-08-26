#![feature(allocator_api)]

use core::slice;
use std::{
	alloc::{Allocator, Global, Layout},
	cell::Cell,
	marker::PhantomData,
	ptr::{self, NonNull},
};

struct Idx<'a, T>(u32, PhantomData<&'a T>);

struct Arena<A: Allocator = Global> {
	ptr: Cell<NonNull<u8>>,
	// buffer capacity
	cap: Cell<u32>,
	// offset from the start
	len: Cell<u32>,
	alloc: A,
}

const INITIAL_CAPACITY: u32 = 4;

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
		let layout = Layout::array::<u8>(INITIAL_CAPACITY as usize)
			.unwrap()
			.align_to(4)
			.unwrap();

		let ptr = alloc.allocate(layout).unwrap().cast();

		Some(Self {
			ptr: Cell::new(ptr),
			cap: Cell::new(INITIAL_CAPACITY),
			len: Cell::new(0),
			alloc,
		})
	}

	pub fn alloc<T>(&self, val: T) -> Idx<'_, T> {
		self.try_alloc(val).unwrap()
	}

	pub fn try_alloc<'a, T>(&'a self, val: T) -> Option<Idx<'a, T>> {
		let layout = Layout::new::<T>();
		let layout_size = u32::try_from(layout.size()).ok()?;

		let ptr = self.ptr.get();
		let cap = self.cap.get();
		let len = self.len.get();

		let next_len = len.checked_add(layout_size)?;

		// reallocate
		if next_len >= cap {
			let next_cap = cap.checked_mul(2)?;

			let layout = Layout::array::<u8>(next_cap as usize)
				.unwrap()
				.align_to(4)
				.unwrap();

			let next_ptr = self.alloc.allocate(layout).unwrap().cast::<u8>();

			// SAFETY: ?
			unsafe {
				ptr::copy_nonoverlapping::<u8>(ptr.as_ptr(), next_ptr.as_ptr(), len as usize)
			};

			// SAFETY: ?
			unsafe {
				let next_layout = Layout::array::<u8>(cap as usize)
					.unwrap()
					.align_to(4)
					.unwrap();

				self.alloc.deallocate(ptr, next_layout);
			}

			self.ptr.set(next_ptr);
			self.cap.set(next_cap);
		}

		// TODO: if layout is very large, we have issue :)

		// TODO: check
		// SAFETY:
		// - len is always smaller than capacity
		// - write is in allocation bounds because next_len is smaller than capacity
		unsafe {
			let ptr = self.ptr.get().add(len as usize);
			ptr::write(ptr.as_ptr().cast(), val);
		};

		self.len.set(next_len);
		Some(Idx(len, PhantomData))
	}

	pub fn get<'a, T>(&'a self, idx: Idx<'a, T>) -> &'a T {
		self.try_get(idx).unwrap()
	}

	pub fn try_get<'a, T>(&'a self, idx: Idx<'a, T>) -> Option<&'a T> {
		let offset = usize::try_from(idx.0).ok()?;
		let start = self.ptr.get();
		let current = unsafe { start.add(offset).cast().as_ref() };
		Some(current)
	}
}

impl<A: Allocator> Arena<A> {
	pub fn as_slice(&self) -> &[u8] {
		unsafe { slice::from_raw_parts(self.ptr.get().as_ptr(), self.len.get() as usize) }
	}
}

impl<A: Allocator> Drop for Arena<A> {
	fn drop(&mut self) {
		let ptr = self.ptr.get();
		let layout = Layout::array::<u8>(self.cap.get() as usize)
			.unwrap()
			.align_to(4)
			.unwrap();

		unsafe {
			self.alloc.deallocate(ptr, layout);
		}
	}
}

#[cfg(test)]
mod tests {
	use super::Arena;

	#[test]
	fn full_circle() {
		let arena = Arena::new();

		for _ in 0..1000 {
			_ = arena.alloc(1);
		}

		let h = arena.alloc(3);
		let v = arena.get(h);

		assert_eq!(*v, 3);
	}

	#[test]
	fn large_item() {
		#[repr(align(2048))]
		struct Foo(u32);

		let arena = Arena::new();

		let h = arena.alloc(Foo(1));
		let v = arena.get(h);

		assert_eq!(v.0, 1);
	}
}
