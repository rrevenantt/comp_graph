use std::borrow::Cow;
use std::cell::{Cell, RefCell};
use std::marker::PhantomData;
use std::rc::Rc;

pub struct OperationNodeInner<T, Op: Operation + ?Sized> {
    cache: Cell<Option<T>>,
    dependents: RefCell<Vec<Rc<dyn Cached>>>,
    operation: Op,
}

// workaround for a compiler limitation when doing trait object casting
// because currently unsizing works only if the last type involves `Op`.
// and using `Op::Output` is considered as being involved
// despite not affecting actual usizing process.
/// Node of computational graph. Supports heterogenous node types.
pub type OperationNode<Op> = OperationNodeInner<<Op as Operation>::Output, Op>;

/// Type erased node of computational graph.
pub type OperationNodeDyn<T> = Rc<OperationNode<dyn Operation<Output = T>>>;

impl<Op: Operation + ?Sized> OperationNode<Op> {
    /// Computes and returns result of computational graph with root at this node.
    pub fn compute(&self) -> Op::Output {
        let cache = self.cache.clone().into_inner();

        cache.unwrap_or_else(|| {
            let new = self.operation.execute();
            self.cache.set(Some(new));
            new
        })
    }
}
impl<Op: Operation> OperationNode<Op> {
    /// Creates new node with `operation`.
    pub fn new(operation: Op) -> Rc<Self> {
        let out = Rc::new(OperationNode {
            cache: Cell::new(None),
            dependents: RefCell::new(vec![]),
            operation,
        });
        let as_dep = out.clone() as Rc<dyn Cached>;
        out.operation.notify_deps(as_dep);
        out
    }
}

/// Input node of computational graph
pub type InputNode<T> = OperationNode<InputOp<T>>;

impl<T: Copy + 'static> InputNode<T> {
    /// Creates new input node
    pub fn new_input(name: impl Into<Cow<'static, str>>) -> Rc<Self> {
        Rc::new(Self {
            operation: InputOp(name.into(), PhantomData),
            cache: Cell::new(None),
            dependents: Default::default(),
        })
    }

    /// Set new value for this input
    pub fn set(&self, data: T) {
        self.invalidate_cache();
        self.cache.set(Some(data));
    }
}

/// Helper for easier creating of new node for unary operation
pub fn new_unary<Prev: ?Sized + Operation, Out: Copy>(
    arg: Rc<OperationNode<Prev>>,
    f: impl 'static + Fn(Prev::Output) -> Out,
) -> Rc<OperationNode<impl Operation<Output = Out>>> {
    OperationNode::new(((arg,), move |(x,)| f(x)))
}

/// Helper for easier creating of new node for binary operation
pub fn new_binary<Prev1: ?Sized + Operation, Prev2: ?Sized + Operation, Out: Copy>(
    arg1: Rc<OperationNode<Prev1>>,
    arg2: Rc<OperationNode<Prev2>>,
    f: impl 'static + Fn(Prev1::Output, Prev2::Output) -> Out,
) -> Rc<OperationNode<impl Operation<Output = Out>>> {
    OperationNode::new(((arg1, arg2), move |(x1, x2)| f(x1, x2)))
}

/// Trait for operations to be supported by computational graph
///
/// Implement it if you want
pub trait Operation: 'static {
    type Output: Copy;
    fn execute(&self) -> Self::Output;

    /// Adds a dependent node to all our dependencies
    fn notify_deps(&self, current: Rc<dyn Cached>);
}

/// Noop operation to indicate input node
pub struct InputOp<T>(Cow<'static, str>, PhantomData<T>);

impl<T: Copy + 'static> Operation for InputOp<T> {
    type Output = T;

    fn execute(&self) -> Self::Output {
        panic!("input data has not been set for {}", self.0);
    }

    fn notify_deps(&self, _current: Rc<dyn Cached>) {}
}

impl<T: Copy + 'static, F, O: Copy> Operation
    for (Vec<Rc<OperationNode<dyn Operation<Output = T>>>>, F)
where
    F: 'static + Fn(Vec<T>) -> O,
{
    type Output = O;

    fn execute(&self) -> Self::Output {
        self.1(self.0.iter().map(|x| x.compute()).collect())
    }

    fn notify_deps(&self, current: Rc<dyn Cached>) {
        for x in &self.0 {
            x.dependents.borrow_mut().push(current.clone());
        }
    }
}
/// Implements [`Operation`] for multiple statically known inputs
macro_rules! impl_tuples {
    ($token:ident $id:tt $($tail:tt)*) => {
        impl_tuple!{ $token $id $($tail)* }
        impl_tuples!{ $($tail)* }
    };
    () => {};
}

macro_rules! impl_tuple {
    ($($generics:ident $ids:tt)+) => {
        impl<$($generics : ?Sized + Operation ),+,F,O: Copy> Operation for ( ($(Rc<OperationNode<$generics>>,)+) , F)
        where
            F: 'static + Fn(( $($generics :: Output ,)+ )) -> O
        {
            type Output = O;

            fn execute(&self) -> Self::Output {
                self.1( reverse!( self [$($ids)+]  ) )
            }

            fn notify_deps(&self, current: Rc<dyn Cached>) {
                $(
                    self.0.$ids.dependents.borrow_mut().push(current.clone());
                )+

            }
        }
    };
}

macro_rules! reverse {
    ($self:ident [] $($reversed:tt)*) => {
        ( $($self.0.$reversed.compute(),)*)
    };
    ($self:ident [$first:tt $($rest:tt)*] $($reversed:tt)*) => {
        reverse!($self [$($rest)*] $first $($reversed)*)
    };
}

impl_tuples!(D 3 C 2 B 1 A 0);

pub trait Cached {
    fn invalidate_cache(&self);
}

impl<Op: Operation> Cached for OperationNode<Op> {
    fn invalidate_cache(&self) {
        self.cache.set(None);
        for x in self.dependents.borrow().iter() {
            x.invalidate_cache();
        }
    }
}

mod tests {
    use super::*;
    use std::ops::Add;

    fn add(
        arg1: Rc<OperationNode<impl Operation<Output = f32>>>,
        arg2: Rc<OperationNode<impl Operation<Output = f32>>>,
    ) -> Rc<OperationNode<impl Operation<Output = f32>>> {
        new_binary(arg1, arg2, f32::add)
    }

    #[test]
    fn test_unary() {
        let x1 = InputNode::new_input("x1");
        x1.set(1);

        let result = new_unary(x1.clone(), |x: i32| x.pow(2));
        assert_eq!(result.compute(), 1);
        x1.set(2);
        assert_eq!(result.cache.clone().into_inner(), None);
        assert_eq!(result.compute(), 4);
    }

    #[test]
    fn test_heterogeneous() {
        let x1 = InputNode::new_input("x1");
        let x2 = InputNode::new_input("x2");
        x1.set(1.5f32);
        x2.set(2i32);

        let result = new_binary(x1.clone(), x2.clone(), f32::powi);
        assert_eq!(result.compute(), 2.25f32);
    }

    #[test]
    fn test_cache() {
        let x1 = InputNode::new_input("x1");
        let x2 = InputNode::new_input("x2");
        let x3 = InputNode::new_input("x3");
        let x4 = InputNode::new_input("x4");
        x1.set(1.0f32);
        x2.set(1.0f32);
        x3.set(1.0f32);
        x4.set(1.0f32);

        let node1 = add(x3.clone(), x4.clone());
        let node2 = add(x2.clone(), node1.clone());
        let node3 = add(x1.clone(), node2.clone());

        assert_eq!(node3.compute(), 4.0f32);

        x2.set(2.0);

        assert_eq!(node1.cache.clone().into_inner(), Some(2.0f32));
        assert_eq!(node2.cache.clone().into_inner(), None);
        assert_eq!(node3.cache.clone().into_inner(), None);

        assert_eq!(node2.compute(), 4.0f32);
        assert_eq!(node1.cache.clone().into_inner(), Some(2.0f32));
        assert_eq!(node2.cache.clone().into_inner(), Some(4.0f32));
        assert_eq!(node3.cache.clone().into_inner(), None);
    }

    #[test]
    fn test_runtime_and_unsizing() {
        let inputs = (0..5)
            .map(|x| {
                let input = InputNode::new_input(x.to_string());
                input.set(x);
                input as OperationNodeDyn<i32>
            })
            .collect::<Vec<_>>();

        let result = OperationNode::new((inputs, |x: Vec<i32>| x.iter().sum::<i32>()))
            as OperationNodeDyn<i32>;

        assert_eq!(result.compute(), 10);

        let result = new_unary(result, |x| x + 2);
        assert_eq!(result.compute(), 12);
    }
}
