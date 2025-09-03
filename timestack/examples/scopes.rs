// This is a fake analysis of the following program
//
// ```
// var one = 1
// var two = 2
// {
//   var three = on // intentional typo
// }
// var four = two
// ```

use std::collections::HashMap;

use distance::levenshtein;
use timestack::{Time, TimeStack};

type Symbol = &'static str;

fn main() {
	let mut ctx = Context::default();

	// Fake tree analysis
	analysis(&mut ctx);

	// Assume we found that `on` doesn't exist
	// We want to diagnostic that and maybe find some close matches
	//
	// Try to edit the target to: `one`, `tw`, `four`
	let target_to_access = "on";

	// We do all theses searches at the scope time of var definition three
	let time_at_three = ctx.vars.get("three").unwrap();

	if ctx
		.stack
		.iter_at(*time_at_three)
		.any(|s| *s == target_to_access)
	{
		println!("Found `{target_to_access}` in scope!");
	} else {
		let mut same = ctx
			.stack
			.iter_at(*time_at_three)
			.filter(|item| levenshtein(item, target_to_access) < 3)
			.collect::<Vec<_>>();
		same.sort();

		println!("We could not find `{target_to_access}` in scope, maybe you meant {same:?}?");
	}
}

#[derive(Debug, Default)]
struct Context {
	stack: TimeStack<Symbol>,
	vars: HashMap<Symbol, Time>,
}

// Fake a tree traversal
fn analysis(ctx: &mut Context) {
	ctx.stack.push("one");
	ctx.vars.insert("one", ctx.stack.time());

	ctx.stack.push("two");
	ctx.vars.insert("two", ctx.stack.time());

	let pre_block_time = ctx.stack.time();
	{
		ctx.stack.push("three");
		ctx.vars.insert("three", ctx.stack.time());
	}
	ctx.stack.warp(pre_block_time);

	ctx.stack.push("four");
	ctx.vars.insert("four", ctx.stack.time());
}
