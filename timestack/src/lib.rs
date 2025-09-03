#[derive(Debug)]
pub struct TimeStack<T> {
	items: Vec<StackItem<T>>,
}

/// Time refers to the index you need to start to read the stack from.
///
/// A Time of 0 refers to the origin, before any element were added.
///
/// The first element can be added with a time of 1.
#[derive(Debug, Clone, Copy)]
pub struct Time(usize);

#[derive(Debug)]
enum StackItem<T> {
	Item(T),
	Ref(usize),
}

impl<T> Default for TimeStack<T> {
	fn default() -> Self {
		Self {
			items: Vec::default(),
		}
	}
}

impl<T> TimeStack<T> {
	pub fn new() -> Self {
		Self::default()
	}

	pub fn time(&self) -> Time {
		Time(self.items.len())
	}

	pub fn push(&mut self, item: T) {
		self.items.push(StackItem::Item(item));
	}

	#[track_caller]
	pub fn warp(&mut self, time: Time) {
		debug_assert!(
			time.0 < self.time().0,
			"you cannot warp to the present or the future, make sure to not mixup times from different `TimeStack`s"
		);
		self.items.push(StackItem::Ref(time.0));
	}
}

impl<T> TimeStack<T> {
	pub fn iter_at(&self, time: Time) -> Iter<'_, T> {
		Iter::new(&self.items[..time.0])
	}
}

#[derive(Debug, Clone)]
pub struct Iter<'a, T> {
	stack: &'a [StackItem<T>],
	idx: usize,
}

impl<'a, T> Iter<'a, T> {
	fn new(view: &'a [StackItem<T>]) -> Self {
		Self {
			stack: view,
			idx: view.len().saturating_sub(0),
		}
	}
}

impl<'a, T> Iterator for Iter<'a, T> {
	type Item = &'a T;
	fn next(&mut self) -> Option<Self::Item> {
		while self.idx != 0 {
			match &self.stack[self.idx - 1] {
				StackItem::Item(item) => {
					self.idx -= 1;
					return Some(item);
				}
				StackItem::Ref(idx) => self.idx = *idx,
			}
		}
		None
	}
}

pub struct ReadOnlyStack<T> {
	items: Box<[T]>,
}

#[cfg(test)]
mod tests {
	use super::*;

	#[track_caller]
	fn assert_iter<T: std::fmt::Debug + PartialEq>(iter: impl Iterator<Item = T>, slice: &[T]) {
		let vec = iter.collect::<Vec<_>>();
		assert_eq!(vec.as_slice(), slice)
	}

	/// Modelize the following scope
	///
	/// ```
	/// var foo
	/// var bar
	/// {
	///   var baz
	/// }
	/// var toto
	/// ```
	#[test]
	fn scopes() {
		let mut stack = TimeStack::default();

		stack.push("foo");
		stack.push("bar");

		let pre_block_time = stack.time();
		stack.push("baz");
		stack.warp(pre_block_time);

		stack.push("todo");

		// List vars in scope at the current time
		assert_iter(
			stack.iter_at(stack.time()).cloned(),
			&["todo", "bar", "foo"],
		);
		// List vars in scope before we entered the block
		assert_iter(stack.iter_at(pre_block_time).cloned(), &["bar", "foo"]);
	}

	#[test]
	#[should_panic]
	fn warp_to_the_future() {
		let mut stack_one = TimeStack::default();
		for _ in 0..3 {
			stack_one.push(0);
		}
		let future = stack_one.time();

		let mut stack_two = TimeStack::<u8>::default();
		stack_two.push(0);

		// Should panic here
		stack_two.warp(future);
	}
}
