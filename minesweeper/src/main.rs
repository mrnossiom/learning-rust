use std::{
	collections::VecDeque,
	io::{self, Read, Write},
	mem::MaybeUninit,
	os::fd::{AsFd, AsRawFd},
};

use calloop::{
	EventLoop, Interest, PostAction,
	generic::Generic,
	signals::{Signal, Signals},
};
use nix::{ioctl_read_bad, pty::Winsize};

use crate::{
	grid::{GridOptions, UVec2, refs},
	tty::{Action, ctrlseq},
};

mod grid;
mod tty;

struct State {
	grid: grid::Grid,

	pending_actions: VecDeque<tty::Action>,
	lock_controls: bool,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
	let grid = grid::Grid::random(GridOptions {
		size: UVec2::new(20, 10),
		nb_bombs: 30,
	});

	let mut state = State {
		grid,
		pending_actions: Default::default(),
		lock_controls: false,
	};

	let tty = tty::PosixTty::new();
	let stdin_fd = tty.as_fd().as_raw_fd();
	let mut stdout = io::stdout().lock();

	write!(
		stdout,
		"{}{}{}",
		ctrlseq::ALT_SCREEN_ENTER,
		ctrlseq::MOUSE_ENABLE,
		ctrlseq::ERASE_DISPLAY
	)?;

	let mut lp = EventLoop::<State>::try_new()?;

	lp.handle().insert_source(
		Generic::new(tty, Interest::READ, calloop::Mode::Level),
		|rdy, stdin, state| {
			if rdy.readable {
				let mut buf = [0u8; 128];
				let stdin = unsafe { stdin.get_mut() };
				let nread = stdin.file.read(&mut buf).unwrap();
				match &buf[..nread] {
					[kp] => {
						state.pending_actions.push_back(Action::KeyPress(*kp));
					}
					[0x1b, b'[', b'M', button, x, y] => {
						state.pending_actions.push_back(Action::Mouse(
							match button & 0b11 {
								0 => tty::MouseButton::Left,
								1 => tty::MouseButton::Middle,
								2 => tty::MouseButton::Right,
								3 => tty::MouseButton::Release,
								_ => unreachable!(),
							},
							UVec2::new(*x as u32 - 32, *y as u32 - 32),
						));
					}
					buf => {
						eprintln!("{buf:?}");
					}
				};
			}
			Ok(PostAction::Continue)
		},
	)?;

	lp.handle()
		.insert_source(Signals::new(&[Signal::SIGWINCH])?, |ev, _, state| {
			if ev.signal() == Signal::SIGWINCH {
				let winsize = term_get_winsize(stdin_fd).unwrap();
				state
					.pending_actions
					.push_back(Action::WindowSizeChange(UVec2::new(
						winsize.ws_col as u32,
						winsize.ws_row as u32,
					)));
			}
		})?;

	loop {
		let State {
			grid,
			pending_actions,
			lock_controls,
		} = &mut state;

		if grid.nb_bombs == grid.nb_flags {
			break;
		}

		for action in pending_actions.drain(..) {
			match action {
				Action::KeyPress(b'q') => return Ok(()),
				Action::WindowSizeChange(size) => {
					*lock_controls = !size.includes(grid.size);
				}
				Action::Mouse(button, pos) => {
					if *lock_controls {
						continue;
					}

					let Some(pos) = pos.try_transform::<refs::Grid>(|x, y| {
						let (x, y) = ((x - 1) / 2, y - 1);
						// exclude out of bounds
						grid.size.in_bounds(UVec2::new(x, y)).then_some((x, y))
					}) else {
						continue;
					};

					let idx = pos.as_idx(grid.size, refs::Grid).unwrap() as usize;
					match button {
						tty::MouseButton::Left => grid.cell_uncover(idx, true),
						tty::MouseButton::Right => grid.cell_toggle_flag(idx),
						_ => {}
					}
				}
				_ => {}
			}
		}

		if grid.exploded {
			break;
		}

		write!(stdout, "{}{}", ctrlseq::ERASE_DISPLAY, ctrlseq::CURSOR_HOME)?;

		if *lock_controls {
			write!(
				stdout,
				"Window size is too small. Need at least {}x{}.",
				grid.size.x, grid.size.y
			)?;
		} else {
			write!(stdout, "{grid}")?;
			write!(
				stdout,
				"{} bombs remaining, {} flags\r\n",
				grid.nb_bombs - grid.nb_flags,
				grid.nb_flags
			)?;
		}

		stdout.flush()?;

		lp.dispatch(None, &mut state)?;
	}

	write!(
		stdout,
		"{}{}",
		ctrlseq::ALT_SCREEN_EXIT,
		ctrlseq::MOUSE_DISABLE
	)?;

	drop(lp);

	let State { grid, .. } = &mut state;

	if grid.exploded {
		writeln!(stdout, "You lost!")?;
	} else {
		writeln!(stdout, "You won!")?;
	}

	Ok(())
}

ioctl_read_bad!(term_get_winsize_raw, nix::libc::TIOCGWINSZ, Winsize);

pub fn term_get_winsize(fd: i32) -> nix::Result<Winsize> {
	let mut winsize = MaybeUninit::<Winsize>::uninit();
	unsafe { term_get_winsize_raw(fd, winsize.as_mut_ptr())? };
	unsafe { Ok(winsize.assume_init()) }
}
