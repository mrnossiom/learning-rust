use fake::{Dummy, Fake};
use indicatif::{ProgressBar, ProgressStyle};

#[derive(Debug, PartialEq, Eq, Dummy)]
struct Birthday(#[dummy(faker = "1..=365")] pub u16);

const GROUPS_SIZE: u64 = 1_000;
const P50_SIZE: usize = 23;
const P99_SIZE: usize = 57;

fn main() {
	let group_size = std::env::var("BIG_GROUP")
		.map(|_| P99_SIZE)
		.unwrap_or(P50_SIZE);

	let mut counter_pos = 0;

	let bar = ProgressBar::new(GROUPS_SIZE).with_style(
		ProgressStyle::with_template("[{elapsed_precise}] {wide_bar} {pos:>7}/{len:7} {msg}")
			.unwrap(),
	);
	'outer: for i in 1..=GROUPS_SIZE {
		bar.inc(1);
		bar.set_message(format!("{}%", counter_pos * 100 / i));

		let group = fake::vec![Birthday; group_size];

		for (ii, birthday) in group.iter().enumerate() {
			for (iii, other_birthday) in group.iter().enumerate() {
				if ii == iii {
					continue;
				}

				if birthday == other_birthday {
					counter_pos += 1;
					continue 'outer;
				}
			}
		}
	}

	bar.finish();
	dbg!(counter_pos);
}
