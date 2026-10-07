use std::{fmt, marker::PhantomData, mem};

use rand::{Rng, RngExt, rngs::ThreadRng};

#[derive(Debug)]
pub struct UVec2<Ref = refs::Any> {
	pub x: u32,
	pub y: u32,
	_ref: PhantomData<Ref>,
}

impl<R> Clone for UVec2<R> {
	fn clone(&self) -> Self {
		*self
	}
}

impl<R> Copy for UVec2<R> {}

impl<Ref> UVec2<Ref> {
	#[inline(always)]
	pub fn new(x: u32, y: u32) -> Self {
		Self {
			x,
			y,
			_ref: PhantomData,
		}
	}

	fn area(&self) -> u32 {
		self.x * self.y
	}

	pub(crate) fn includes(&self, size: Self) -> bool {
		size.x <= self.x && size.y <= self.y
	}

	pub(crate) fn in_bounds(&self, size: Self) -> bool {
		size.x < self.x && size.y < self.y
	}

	pub fn transform<New>(&self, f: impl FnOnce(u32, u32) -> (u32, u32)) -> UVec2<New> {
		let (x, y) = f(self.x, self.y);
		UVec2::new(x, y)
	}

	pub fn try_transform<New>(
		&self,
		f: impl FnOnce(u32, u32) -> Option<(u32, u32)>,
	) -> Option<UVec2<New>> {
		let (x, y) = f(self.x, self.y)?;
		Some(UVec2::new(x, y))
	}

	pub fn as_idx(&self, size: UVec2, referential: Ref) -> Option<u32> {
		_ = referential;
		let UVec2 { x, y, .. } = *self;
		let UVec2 {
			x: width,
			y: height,
			..
		} = size;

		// if out of bounds
		if x >= width || y >= height {
			return None;
		};
		Some(y * width + x)
	}
}

pub(crate) mod refs {
	pub(crate) struct Any;
	pub(crate) struct Grid;
	pub(crate) struct Window;
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
enum CellState {
	#[default]
	Covered,
	Uncovered,
	Flagged,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum CellContent {
	Bomb,
	Empty { nb_neighbour_bombs: u8 },
}

impl Default for CellContent {
	fn default() -> Self {
		Self::Empty {
			nb_neighbour_bombs: 0,
		}
	}
}

impl fmt::Display for CellContent {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		match self {
			CellContent::Bomb => write!(f, "*"),
			CellContent::Empty { nb_neighbour_bombs } => match nb_neighbour_bombs {
				0 => write!(f, " "),
				nb @ 1..=8 => write!(f, "{nb}"),
				_ => unreachable!(),
			},
		}
	}
}

#[derive(Default, Clone, Copy)]
pub struct Cell {
	state: CellState,
	content: CellContent,
}

impl fmt::Display for Cell {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		match self.state {
			CellState::Flagged => write!(f, "~"),
			CellState::Covered => write!(f, "-"),
			CellState::Uncovered => write!(f, "{}", self.content),
		}
	}
}

pub(crate) struct Grid {
	pub cells: Vec<Cell>,
	pub size: UVec2,

	pub nb_bombs: u32,
	pub nb_flags: u32,
	pub nb_uncovered: u32,

	pub exploded: bool,
}

pub(crate) struct GridOptions {
	pub size: UVec2,
	pub nb_bombs: u32,
}

impl Grid {
	pub fn random(options: GridOptions) -> Self {
		let mut thread_rng = ThreadRng::default();
		Self::new(options, &mut thread_rng)
	}

	pub fn new(options: GridOptions, rng: &mut impl Rng) -> Self {
		let GridOptions { size, nb_bombs } = options;
		assert!(nb_bombs <= size.area());

		let mut cells = vec![Cell::default(); size.area() as usize];

		{
			let mut nb_bombs = nb_bombs;
			while nb_bombs != 0 {
				let idx = rng.random_range(0..cells.len());
				let cell = &mut cells[idx];
				let old_content = mem::replace(&mut cell.content, CellContent::Bomb);
				if old_content != CellContent::Bomb {
					nb_bombs -= 1;
				}
			}
		}

		for cell_idx in 0..cells.len() {
			if cells[cell_idx].content == CellContent::Bomb {
				for neighbor_idx in cell_neighbors(cell_idx, size) {
					let CellContent::Empty { nb_neighbour_bombs } =
						&mut cells[neighbor_idx].content
					else {
						continue;
					};

					*nb_neighbour_bombs += 1;
				}
			}
		}

		Self {
			cells,
			size,
			nb_bombs,
			nb_flags: 0,
			nb_uncovered: 0,
			exploded: false,
		}
	}

	pub fn cell_uncover(&mut self, idx: usize, recurse: bool) {
		let Cell { state, content } = &mut self.cells[idx];
		let uncover_neighbors = match state {
			CellState::Covered => {
				*state = CellState::Uncovered;
				self.nb_uncovered += 1;

				if *content == CellContent::Bomb {
					self.exploded = true;
				}

				matches!(
					content,
					CellContent::Empty {
						nb_neighbour_bombs: 0
					}
				)
			}
			CellState::Uncovered => recurse,
			CellState::Flagged => false,
		};

		if uncover_neighbors {
			for neighbor_idx in cell_neighbors(idx, self.size) {
				self.cell_uncover(neighbor_idx, false);
			}
		}
	}

	pub fn cell_toggle_flag(&mut self, idx: usize) {
		let Cell { state, .. } = &mut self.cells[idx];
		match state {
			CellState::Covered => {
				*state = CellState::Flagged;
				self.nb_flags += 1;
			}
			CellState::Uncovered => {}
			CellState::Flagged => {
				*state = CellState::Covered;
				self.nb_flags -= 1;
			}
		}
	}
}

fn cell_neighbors(cell_idx: usize, size: UVec2) -> impl Iterator<Item = usize> {
	let first_col = cell_idx.is_multiple_of(size.x as usize);
	let last_col = cell_idx % size.x as usize == (size.x as usize - 1);
	let first_row = cell_idx < size.x as usize;
	let last_row = cell_idx >= size.x as usize * (size.y as usize - 1);

	let indices = [
		(!first_col).then(|| cell_idx - 1),
		(!last_col).then(|| cell_idx + 1),
		(!first_row).then(|| cell_idx - size.x as usize),
		(!last_row).then(|| cell_idx + size.x as usize),
		(!first_col && !first_row).then(|| cell_idx - 1 - size.x as usize),
		(!first_col && !last_row).then(|| cell_idx - 1 + size.x as usize),
		(!last_col && !first_row).then(|| cell_idx + 1 - size.x as usize),
		(!last_col && !last_row).then(|| cell_idx + 1 + size.x as usize),
	];

	indices.into_iter().flatten()
}

impl fmt::Display for Grid {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		for row in self.cells.chunks(self.size.x as usize) {
			for (i, cell) in row.iter().enumerate() {
				if i != 0 {
					write!(f, " ")?;
				}
				write!(f, "{cell}")?;
			}
			write!(f, "\r\n")?;
		}
		Ok(())
	}
}
