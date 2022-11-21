use smallvec::SmallVec;
use std::borrow::Cow;
use std::cmp::Reverse;
use std::collections::HashMap;

#[derive(Default)]
pub struct CompGraph<T> {
    nodes: Vec<Node<T>>,
    graph_inputs: HashMap<Cow<'static, str>, usize>,
}

// using SmallVec to optimize for binary and unary operations
struct Node<T> {
    cache: Option<T>,
    node_inputs: SmallVec<[NodeId; 2]>,
    dependents: SmallVec<[usize; 2]>,
    op: Box<dyn FnMut(&mut dyn Iterator<Item = T>) -> T>,
}

// for type safety
#[derive(Copy, Clone, Debug)]
pub struct NodeId(usize);

impl<T> CompGraph<T> {
    pub fn new() -> Self {
        Self {
            nodes: vec![],
            graph_inputs: Default::default(),
        }
    }

    pub fn add_node(
        &mut self,
        inputs: impl IntoIterator<Item = NodeId>,
        op: impl 'static + FnMut(&mut dyn Iterator<Item = T>) -> T,
    ) -> NodeId {
        let node_inputs: SmallVec<[NodeId; 2]> = inputs.into_iter().collect();
        let next_id = self.nodes.len();
        for input in node_inputs.iter() {
            self.nodes[input.0].dependents.push(next_id)
        }
        self.nodes.push(Node {
            cache: None,
            node_inputs,
            dependents: SmallVec::new(),
            op: Box::new(op),
        });

        NodeId(next_id)
    }

    pub fn add_input_node(&mut self, name: impl Into<Cow<'static, str>>) -> NodeId {
        let id = self.add_node(Vec::new(), |_| {
            unreachable!("should not be called on input node")
        });
        self.graph_inputs.insert(name.into(), id.0);
        id
    }

    pub fn set_input(&mut self, name: &str, data: T) {
        let input_id = *self.graph_inputs.get(name).expect("no such input");
        self.invalidate_node(NodeId(input_id));
        self.nodes[input_id].cache = Some(data);
    }

    pub fn invalidate_node(&mut self, node: NodeId) {
        let mut stack = vec![Reverse(node.0)];
        while let Some(Reverse(next)) = stack.pop() {
            self.nodes[next].cache = None;
            stack.extend(self.nodes[next].dependents.iter().map(|&x| Reverse(x)))
        }
    }
}
impl<T: Clone> CompGraph<T> {
    #[cfg(test)]
    fn cache(&self, node: NodeId) -> Option<T> {
        self.nodes[node.0].cache.clone()
    }

    pub fn compute(&mut self, node: NodeId) -> T {
        let node = [node];
        calculate_node(&mut self.nodes, &node, &mut |x| x.next().unwrap())
    }
}

fn calculate_node<T: Clone>(
    head: &mut [Node<T>],
    inputs: &[NodeId],
    operation: &mut dyn FnMut(&mut dyn Iterator<Item = T>) -> T,
) -> T {
    for &NodeId(input) in inputs {
        if head[input].cache.is_none() {
            let (before, after) = head.split_at_mut(input);
            let result = calculate_node(before, &after[0].node_inputs, &mut after[0].op);
            head[input].cache = Some(result);
        }
    }
    let mut iter = inputs.iter().map(|input| {
        head[input.0]
            .cache
            .clone()
            .expect("should be set in above loop")
    });
    operation(&mut iter)
}

#[cfg(test)]
mod test {
    use crate::comp_graph3::CompGraph;

    #[test]
    fn test_simple() {
        let mut graph = CompGraph::new();
        let x1 = graph.add_input_node("x1");
        graph.set_input("x1", 1i32);

        let result = graph.add_node([x1], |inputs| inputs.next().unwrap().pow(2));

        assert_eq!(graph.compute(result), 1);
        graph.set_input("x1", 2);
        assert_eq!(graph.compute(result), 4);
    }

    #[test]
    fn test_caches() {
        let mut graph = CompGraph::new();
        let x1 = graph.add_input_node("x1");
        let x2 = graph.add_input_node("x2");
        let x3 = graph.add_input_node("x3");
        let x4 = graph.add_input_node("x4");
        graph.set_input("x1", 1.0f32);
        graph.set_input("x2", 1.0f32);
        graph.set_input("x3", 1.0f32);
        graph.set_input("x4", 1.0f32);

        fn add(args: &mut dyn Iterator<Item = f32>) -> f32 {
            args.next().unwrap() + args.next().unwrap()
        }

        let node1 = graph.add_node([x3, x4], add);
        let node2 = graph.add_node([x2, node1], add);
        let node3 = graph.add_node([x1, node2], add);

        assert_eq!(graph.compute(node3), 4.0f32);

        assert_eq!(graph.cache(node1), Some(2.0f32));

        graph.set_input("x2", 2.0f32);

        assert_eq!(graph.cache(node1), Some(2.0f32));
        assert_eq!(graph.cache(node2), None);
        assert_eq!(graph.cache(node3), None);

        assert_eq!(graph.compute(node2), 4.0f32);
        assert_eq!(graph.cache(node1), Some(2.0f32));
        assert_eq!(graph.cache(node2), Some(4.0f32));
        assert_eq!(graph.cache(node3), None);
    }
}
