use std::{
	fs::{File, OpenOptions},
	os::fd::{AsFd, BorrowedFd},
};

use nix::sys::termios::{
	self, ControlFlags, InputFlags, LocalFlags, OutputFlags, Termios, tcgetattr, tcsetattr,
};

use crate::grid::{UVec2, refs};

pub(crate) struct PosixTty {
	pub file: File,
	initial_termios: Termios,
}

impl PosixTty {
	pub fn new() -> Self {
		let file = OpenOptions::new()
			.read(true)
			.write(true)
			.open("/dev/tty")
			.unwrap();
		let initial_termios = make_raw_terminal(file.as_fd()).unwrap();
		Self {
			file,
			initial_termios,
		}
	}
}

impl Drop for PosixTty {
	fn drop(&mut self) {
		restore_terminal(self.file.as_fd(), self.initial_termios.clone());
	}
}

impl AsFd for PosixTty {
	fn as_fd(&self) -> BorrowedFd<'_> {
		self.file.as_fd()
	}
}

fn make_raw_terminal(fd: BorrowedFd<'_>) -> Option<Termios> {
	let initial = tcgetattr(fd).ok()?;

	let mut next = initial.clone();
	next.input_flags.remove(
		InputFlags::BRKINT
			| InputFlags::ICRNL
			| InputFlags::IGNBRK
			| InputFlags::IGNCR
			| InputFlags::INLCR
			| InputFlags::ISTRIP
			| InputFlags::IXON
			| InputFlags::PARMRK,
	);
	next.output_flags.remove(OutputFlags::OPOST);
	next.local_flags.remove(
		LocalFlags::ECHO
			| LocalFlags::ECHONL
			| LocalFlags::ICANON
			| LocalFlags::IEXTEN
			| LocalFlags::ISIG,
	);

	// set character size
	next.control_flags
		.remove(ControlFlags::CSIZE | ControlFlags::PARENB);
	next.control_flags.insert(ControlFlags::CS8);

	tcsetattr(fd, termios::SetArg::TCSAFLUSH, &next).ok()?;

	Some(initial)
}

fn restore_terminal(fd: BorrowedFd<'_>, initial: Termios) {
	_ = tcsetattr(fd, termios::SetArg::TCSAFLUSH, &initial);
}

pub(crate) enum Action {
	KeyPress(u8),
	WindowSizeChange(UVec2),
	Mouse(MouseButton, UVec2<refs::Window>),
}

pub(crate) enum MouseButton {
	Left,
	Middle,
	Right,
	Release,
}

pub(crate) mod ctrlseq {
	#![expect(dead_code)]

	macro_rules! csi {
		($seq:literal) => {
			concat!("\x1b[", $seq)
		};
	}

	pub const ERASE_DISPLAY_CURSOR_TO_END: &str = csi!("0J");
	pub const ERASE_DISPLAY_START_TO_CURSOR: &str = csi!("1J");
	pub const ERASE_DISPLAY: &str = csi!("2J");

	pub const CURSOR_UP: &str = csi!("A");
	pub const CURSOR_DOWN: &str = csi!("B");
	pub const CURSOR_RIGHT: &str = csi!("C");
	pub const CURSOR_LEFT: &str = csi!("D");

	pub const CURSOR_HOME: &str = csi!("H");

	pub const CURSOR_SHOW: &str = csi!("?25h");
	pub const CURSOR_HIDE: &str = csi!("?25l");

	pub const MOUSE_ENABLE: &str = csi!("?1000h");
	pub const MOUSE_DISABLE: &str = csi!("?1000l");

	pub const ALT_SCREEN_ENTER: &str = csi!("?1049h");
	pub const ALT_SCREEN_EXIT: &str = csi!("?1049l");

	pub const COLOR_RESET: &str = csi!("0m");
	pub const COLOR_RED: &str = csi!("31m");
	pub const COLOR_BLUE: &str = csi!("34m");
	pub const COLOR_ORANGE: &str = csi!("35m");
}
