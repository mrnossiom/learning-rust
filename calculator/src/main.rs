use std::{
	env::args,
	io::{stdin, stdout, Write},
};

enum RpnNode {
	Number(i32),
	Op(RpnOp),
}

enum RpnOp {
	Add,
	Substract,
	Multiply,
	Divide,
}

// + 3 - 2 4
fn parse_rpn(input: &str) -> Result<Vec<RpnNode>, String> {
	let nodes = input.split_whitespace();
	let mut stack = vec![];

	for node in nodes {
		if node.is_empty() {
			continue;
		}

		let parsed = match node {
			"+" => RpnNode::Op(RpnOp::Add),
			"-" => RpnNode::Op(RpnOp::Substract),
			"*" => RpnNode::Op(RpnOp::Multiply),
			"/" => RpnNode::Op(RpnOp::Divide),
			_ => match node.parse::<i32>() {
				Ok(n) => RpnNode::Number(n),
				Err(_) => return Err("this expr could not be parsed".into()),
			},
		};

		stack.push(parsed);
	}

	Ok(stack)
}

fn eval_rpn(stack: Vec<RpnNode>) -> i32 {
	let mut nstack = Vec::new();

	for node in stack {
		match node {
			RpnNode::Number(n) => nstack.push(n),
			RpnNode::Op(op) => {
				let n2 = nstack.pop().unwrap();
				let n1 = nstack.pop().unwrap();

				let res = match op {
					RpnOp::Add => n1 + n2,
					RpnOp::Substract => n1 - n2,
					RpnOp::Multiply => n1 * n2,
					RpnOp::Divide => n1 / n2,
				};

				nstack.push(res);
			}
		}
	}

	nstack.pop().unwrap()
}

fn main_rpn() -> Result<(), Box<dyn std::error::Error>> {
	let mut buffer = String::new();

	loop {
		print!("rpn> ");
		stdout().flush()?;

		let len = stdin().read_line(&mut buffer)?;
		if len <= 1 {
			break;
		}

		let parsed = match parse_rpn(buffer.trim()) {
			Ok(stack) => stack,
			Err(msg) => {
				println!("error: {}", msg);
				continue;
			}
		};

		let result = eval_rpn(parsed);
		println!("result: {}", result);
	}

	Ok(())
}

fn parse_expression(input: &str) -> Result<(i64, i64), String> {
	let (left, right) = match input.split_once('+') {
		Some(parts) => parts,
		None => return Err("no plus in expression".into()),
	};

	let left: i64 = match left.trim().parse() {
		Ok(n) => n,
		Err(_) => return Err("left side of the expression is invalid".into()),
	};

	let right = match right.trim().parse() {
		Ok(n) => n,
		Err(_) => return Err("right side of the expression is invalid".into()),
	};

	Ok((left, right))
}

fn main_simple() -> Result<(), Box<dyn std::error::Error>> {
	let mut buffer = String::new();

	loop {
		print!("simple> ");
		stdout().flush()?;

		let len = stdin().read_line(&mut buffer)?;
		if len <= 1 {
			break;
		}

		let (left, right) = match parse_expression(buffer.trim()) {
			Ok(parts) => parts,
			Err(msg) => {
				println!("error: {}", msg);
				continue;
			}
		};

		println!("{} + {} = {}", left, right, left + right);
	}

	Ok(())
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
	match args().nth(1).unwrap_or("simple".into()).as_str() {
		"simple" => main_simple()?,
		"rpn" => main_rpn()?,
		kind => panic!("invalid calc kind {kind}"),
	}

	Ok(())
}
