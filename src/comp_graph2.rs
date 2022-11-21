use std::borrow::Cow;
use std::cell::{Cell, RefCell};

pub struct OperationNode<'a, T, Args, Op> {
    cache: Cell<Option<T>>,
    // can even use qcell::TCell to completely remove runtime cost of interior mutability
    // if performance is critical
    // but it felt too much for a test task
    dependents: RefCell<Vec<&'a dyn Cached>>,
    args: Args,
    operation: Op,
}

impl<'a, T, Args, Op> OperationNode<'a, T, Args, Op> {
    /// Creates new node with `operation`.
    pub fn new(args: Args, operation: Op) -> Self {
        let out = OperationNode {
            cache: Cell::new(None),
            dependents: RefCell::new(vec![]),
            args,
            operation,
        };
        out
    }

    fn cached(&self) -> Option<T>
    where
        T: Copy,
    {
        self.cache.clone().into_inner()
    }
}

/// Input node of computational graph
pub type InputNode<'a, T> = OperationNode<'a, T, (), InputOp>;

impl<T: Copy + 'static> InputNode<'_, T> {
    /// Creates new input node
    pub fn new_input(name: impl Into<Cow<'static, str>>) -> Self {
        Self {
            operation: InputOp(name.into()),
            cache: Cell::new(None),
            dependents: Default::default(),
            args: (),
        }
    }

    /// Set new value for this input
    pub fn set(&self, data: T) {
        self.invalidate_cache();
        self.cache.set(Some(data));
    }
}

// /// Helper for easier creating of new node for unary operation
// pub fn new_unary<'a, T: 'static + Copy, X: Compute<'a>>(
//     arg: X,
//     f: impl 'static + Fn(T) -> T,
// ) -> OperationNode<'a, T, ((X,), impl 'static + Fn((T,)) -> T)> {
//     OperationNode::new(((arg,), move |(x,)| f(x)))
// }
//
// /// Helper for easier creating of new node for binary operation
// pub fn new_binary<'a, T: 'static + Copy>(
//     arg1: impl Compute<'a, Output = T>,
//     arg2: impl Compute<'a, Output = T>,
//     f: impl 'a + Fn(T, T) -> T,
// ) -> impl Compute<'a, Output = T> {
//     OperationNode::new(((arg1, arg2), move |(x1, x2)| f(x1, x2)))
// }

/// Noop operation to indicate input node
pub struct InputOp(Cow<'static, str>);

impl<'a, T: Copy + 'static, Arg, Op> Cached for OperationNode<'a, T, Arg, Op> {
    fn invalidate_cache(&self) {
        self.cache.set(None);
        for x in self.dependents.borrow().iter() {
            (**x).invalidate_cache();
        }
    }
}

impl<'a, T: Copy + 'static> Compute<'a> for OperationNode<'a, T, (), InputOp> {
    type Output = T;

    fn compute(&self) -> Self::Output {
        self.cache
            .clone()
            .into_inner()
            .expect("input should have been set at this point")
    }

    fn notify_deps(&'a self, dependent: &'a dyn Cached) {
        println!("add dep for input {} {:p}", self.operation.0, dependent);
        self.dependents.borrow_mut().push(dependent);
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
        impl<'a,T:'static + Copy,$($generics : Compute<'a> ),+,F,> Compute<'a> for OperationNode<'a,T, ($($generics,)+) , F>
        where
            F: 'a + Fn(( $($generics :: Output ,)+ )) -> T
        {
            type Output = T;

            fn compute(&self) -> Self::Output {
                if let Some(cached) = self.cached(){
                    return cached;
                }
                let updated = (self.operation)( reverse!( self [$($ids)+]  ) );
                self.cache.set(Some(updated));
                updated
            }

            fn notify_deps(&'a self, dependent: &'a dyn Cached){
                self.dependents.borrow_mut().push(dependent);
                $(
                    self.args.$ids.notify_deps(self);
                )+
            }

        }
    };
}

macro_rules! reverse {
    ($self:ident [] $($reversed:tt)*) => {
        ( $($self.args.$reversed.compute(),)*)
    };
    ($self:ident [$first:tt $($rest:tt)*] $($reversed:tt)*) => {
        reverse!($self [$($rest)*] $first $($reversed)*)
    };
}

impl_tuples!(D 3 C 2 B 1 A 0);

pub trait Compute<'a>: Cached {
    type Output;
    fn compute(&self) -> Self::Output;
    fn notify_deps(&'a self, dependent: &'a dyn Cached);
    // fn collect_inputs(&'a self, inputs: &mut InputsMap<'a, Self::Output>);

    /// must be called before all operations on the graph start
    fn create_reverse_deps(&'a self)
    where
        Self: Sized,
    {
        self.notify_deps(&());
    }
}

pub trait Cached {
    fn invalidate_cache(&self);
}
impl Cached for () {
    fn invalidate_cache(&self) {}
}

impl<T: Cached + ?Sized> Cached for &'_ T {
    fn invalidate_cache(&self) {
        (**self).invalidate_cache()
    }
}

impl<'a, T: Compute<'a>> Compute<'a> for &'a T {
    type Output = T::Output;

    fn compute(&self) -> Self::Output {
        (**self).compute()
    }

    fn notify_deps(&'a self, dependent: &'a dyn Cached) {
        (**self).notify_deps(dependent)
    }
}

mod tests {
    use super::*;

    #[test]
    fn test_unary() {
        let x1 = InputNode::new_input("x1");
        x1.set(1);

        // let result = new_unary(&x1, |x: i32| x.pow(2));
        let result = OperationNode::new((&x1,), move |(x,): (i32,)| x.pow(2));
        result.create_reverse_deps();

        assert_eq!(result.compute(), 1);
        x1.set(2);
        // assert_eq!(result.cache.clone().into_inner(), None);
        assert_eq!(result.compute(), 4);
    }

    #[test]
    fn test_heterogeneous() {
        let x1 = InputNode::new_input("x1");
        let x2 = InputNode::new_input("x2");
        x1.set(1.5f32);
        x2.set(2i32);

        let result = OperationNode::new((x1, x2), |(x1, x2): (f32, i32)| x1.powi(x2));
        result.create_reverse_deps();
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

        fn add_tuple(arg: (f32, f32)) -> f32 {
            arg.0 + arg.1
        }

        let node1 = OperationNode::new((&x3, &x4), add_tuple);
        let node2 = OperationNode::new((&x2, &node1), add_tuple);
        let node3 = OperationNode::new((&x1, &node2), add_tuple);
        node3.create_reverse_deps();

        assert_eq!(node3.compute(), 4.0f32);

        assert_eq!(node1.cached(), Some(2.0f32));

        x2.set(2.0);

        assert_eq!(node1.cached(), Some(2.0f32));
        assert_eq!(node2.cached(), None);
        assert_eq!(node3.cached(), None);

        assert_eq!(node2.compute(), 4.0f32);
        assert_eq!(node1.cached(), Some(2.0f32));
        assert_eq!(node2.cached(), Some(4.0f32));
        assert_eq!(node3.cached(), None);
    }
}
