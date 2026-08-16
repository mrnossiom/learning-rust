use clap::Parser;

#[derive(Parser)]
struct Args {
	kind: AlgoKind,

	zoom: Option<f32>,
	panx: Option<f32>,
	pany: Option<f32>,
}

#[derive(Clone, clap::ValueEnum)]
enum AlgoKind {
	Naive,
	Optimized,
	Integers,
}

const PALETTE: &[u8] = b" .'`^\",:;Il!i><~+_-?][}{1)(|\\/tfjrxnuvczXYUJCLQ0OZmwqpdbkhao*#MW&8%B@$";
// const PALETTE: &[u8] = b" `.-':_,^=;><+!rc*/z?sLTv)J7(|Fi{C}fI31tlu[neoZ5Yxjya]2ESwqkP6h9d4VpOGbUAKXHm8RD#$Bg0MNWQ%&@";

fn main() {
	let Args {
		kind,
		zoom,
		panx,
		pany,
	} = Args::parse();

	let (height, width) = (30u32, 120u32);
	let (zoom, pan) = (zoom.unwrap_or(1.), (panx.unwrap_or(0.), pany.unwrap_or(0.)));

	match kind {
		AlgoKind::Naive => mandelbrot_naive((height, width), zoom, pan),
		AlgoKind::Optimized => mandelbrot_optimized((height, width), zoom, pan),
		AlgoKind::Integers => mandelbrot_integers((height, width), zoom, pan),
	}
}

// Naive <https://en.wikipedia.org/wiki/Plotting_algorithms_for_the_Mandelbrot_set#Unoptimized_na%C3%AFve_escape_time_algorithm>
fn mandelbrot_naive((height, width): (u32, u32), zoom: f32, pan: (f32, f32)) {
	let (xscale, yscale) = ((-2., 0.47), (-1.12, 1.12));
	let xscale = ((xscale.0 + pan.0) / zoom, (xscale.1 + pan.0) / zoom);
	let yscale = ((yscale.0 + pan.1) / zoom, (yscale.1 + pan.1) / zoom);

	let max_k = 1000;

	for y in 0..height {
		for x in 0..width {
			let x0 = xscale.0 + x as f32 * (xscale.1 - xscale.0) / width as f32;
			let y0 = yscale.0 + y as f32 * (yscale.1 - yscale.0) / height as f32;
			let (mut x, mut y) = (0., 0.);
			let mut k = 0u32;

			while x * x + y * y < 2. * 2. && k < max_k {
				let xtmp = x * x - y * y + x0;
				y = 2. * x * y + y0;
				x = xtmp;
				k += 1;
			}

			let char_idx = (k as f32 / (max_k as f32 + 1.) * PALETTE.len() as f32) as u32;
			print!("{}", PALETTE[char_idx as usize] as char);
		}
		println!();
	}
}

// <https://en.wikipedia.org/wiki/Plotting_algorithms_for_the_Mandelbrot_set#Optimized_escape_time_algorithms>
fn mandelbrot_optimized((height, width): (u32, u32), zoom: f32, pan: (f32, f32)) {
	let (xscale, yscale) = ((-2., 0.47), (-1.12, 1.12));
	let xscale = ((xscale.0 + pan.0) / zoom, (xscale.1 + pan.0) / zoom);
	let yscale = ((yscale.0 + pan.1) / zoom, (yscale.1 + pan.1) / zoom);

	let max_k = 100;

	for y in 0..height {
		for x in 0..width {
			let x0 = xscale.0 + x as f32 * (xscale.1 - xscale.0) / width as f32;
			let y0 = yscale.0 + y as f32 * (yscale.1 - yscale.0) / height as f32;

			let (mut x, mut y) = (0., 0.);
			let (mut x2, mut y2) = (0., 0.);
			let mut k = 0u32;

			while x2 + y2 < 4. && k < max_k {
				x2 = x * x;
				y2 = y * y;
				y = 2. * x * y + y0;
				x = x2 - y2 + x0;
				k += 1;
			}

			let char_idx = (k as f32 / (max_k as f32 + 1.) * PALETTE.len() as f32) as u32;
			print!("{}", PALETTE[char_idx as usize] as char);
		}
		println!();
	}
}

// Mandelbrot Set with integers <https://graeme-winter.github.io/2023/02/2023-02-15.html>
fn mandelbrot_integers((height, width): (u32, u32), zoom: f32, pan: (f32, f32)) {
	fn mul(a: i32, b: i32) -> i32 {
		(a as i64 * (b as i64 >> 24)) as i32
	}

	fn iter(cr: i32, ci: i32) -> u32 {
		let mut zr = 0;
		let mut zi = 0;

		let mut k = 0;
		let max_k = 4096;

		while k < max_k {
			let zr2 = mul(zr, zr);
			let zi2 = mul(zi, zi);
			if zr2 + zi2 > (4 << 24) {
				break;
			}

			k += 1;

			let tmp = zr;
			zr = zr2 - zi2 + cr;
			zi = 2 * mul(tmp, zi) + ci;
		}

		k
	}

	for y in 0..height as i32 {
		for x in 0..width as i32 {
			let cr = -(2 << 24) + 0x8000 * x + 0x4000;
			let ci = -(5 << 22) + 0x8000 * y + 0x4000;
			let k = iter(cr, ci);

			let char_idx = (k as f32 / (4096. + 1.) * PALETTE.len() as f32) as u32;
			print!("{}", PALETTE[char_idx as usize] as char);
		}
		println!();
	}
}
