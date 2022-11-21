// straightforward version that corresponds to the API of the example in task description
mod comp_graph;
// zero-allocation static dispatch version with heterogeneous nodes if the performance is critical
// and graph is known beforehand
mod comp_graph2;
// most readable and maintainable arena-based version that is fast enough for most cases
mod comp_graph3;

use comp_graph::*;
use std::ops::{Add, Mul};
use std::rc::Rc;

fn create_input(name: &'static str) -> Rc<InputNode<f32>> {
    InputNode::new_input(name)
}

fn add(
    arg1: Rc<OperationNode<impl Operation<Output = f32>>>,
    arg2: Rc<OperationNode<impl Operation<Output = f32>>>,
) -> Rc<OperationNode<impl Operation<Output = f32>>> {
    new_binary(arg1, arg2, f32::add)
}

fn mul(
    arg1: Rc<OperationNode<impl Operation<Output = f32>>>,
    arg2: Rc<OperationNode<impl Operation<Output = f32>>>,
) -> Rc<OperationNode<impl Operation<Output = f32>>> {
    new_binary(arg1, arg2, f32::mul)
}

fn sin(
    arg1: Rc<OperationNode<impl Operation<Output = f32>>>,
) -> Rc<OperationNode<impl Operation<Output = f32>>> {
    new_unary(arg1, f32::sin)
}
fn pow_f32(
    arg1: Rc<OperationNode<impl Operation<Output = f32>>>,
    n: f32,
) -> Rc<OperationNode<impl Operation<Output = f32>>> {
    new_unary(arg1, move |x| x.powf(n))
}

fn round(x: f32, precision: u32) -> f32 {
    let m = 10i32.pow(precision) as f32;
    (x * m).round() / m
}
fn main() {
    // x1, x2, x3 are input nodes of the computational graph:
    let x1 = create_input("x1");
    let x2 = create_input("x2");
    let x3 = create_input("x3");
    // graph variable is the output node of the graph:
    let graph = add(
        x1.clone(),
        mul(x2.clone(), sin(add(x2.clone(), pow_f32(x3.clone(), 3f32)))),
    );
    x1.set(1f32);
    x2.set(2f32);
    x3.set(3f32);
    let mut result = graph.compute();
    result = round(result, 5);
    println!("Graph output = {}", result);
    assert_eq!(round(result, 5), -0.32727);
    x1.set(2f32);
    x2.set(3f32);
    x3.set(4f32);
    result = graph.compute();
    result = round(result, 5);
    println!("Graph output = {}", result);
    assert_eq!(round(result, 5), -0.56656);
}
