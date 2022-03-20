use super::traits::{CanAddTape, IsShapedArray, Tensor, TensorCreator};
use super::{structs::*, TapeManager};
use super::{HasUniqueId, NoTape};
use crate::gradients::{BinaryOp, OpType, Operation};
use crate::prelude::GradientTape;
use ndarray::prelude::Array;
use ndarray::Dimension;

fn binary_op<
    Lhs: HasUniqueId + IsShapedArray,
    Rhs: HasUniqueId + IsShapedArray,
    Out: HasUniqueId + IsShapedArray,
    D1: Dimension,
    D2: Dimension,
>(
    tape: &mut Box<GradientTape>,
    op_type: OpType,
    operands: (&Lhs, &Rhs, &Out),
    lhs_deriv: Array<f32, D1>,
    rhs_deriv: Array<f32, D2>,
) {
    let parent_grads = [
        tape.gradient_ref_for(operands.0.id(), operands.0.shape()),
        tape.gradient_ref_for(operands.1.id(), operands.1.shape()),
    ];
    let parent_derivs = [
        tape.store_derivative(lhs_deriv),
        tape.store_derivative(rhs_deriv),
    ];
    let result_grad = tape.gradient_ref_for(operands.2.id(), operands.2.shape());
    tape.add_operation(Operation::Binary(BinaryOp {
        op_type,
        parent_grads,
        parent_derivs,
        result_grad,
    }));
}

pub fn matmat_mul<const M: usize, const N: usize, const O: usize, Mgr: TapeManager>(
    lhs: Tensor2D<M, N, Mgr>,
    rhs: &Tensor2D<N, O, NoTape>,
) -> Tensor2D<M, O, Mgr> {
    let result = Tensor2D::new(lhs.data().dot(rhs.data()));
    let (lhs, mut tape_manager) = lhs.split_tape_manager();
    tape_manager.update_with(|tape| {
        binary_op(
            tape,
            OpType::MatMul { m: M, n: N, o: O },
            (&lhs, rhs, &result),
            // NOTE: the derivatives here are reversed for matrix multiplication
            rhs.data.clone(),
            lhs.data.clone(),
        )
    });
    result.with_tape_manager(tape_manager)
}

impl<const M: usize, const N: usize, const O: usize, Mgr: TapeManager>
    std::ops::Mul<&Tensor2D<N, O, NoTape>> for Tensor2D<M, N, Mgr>
{
    type Output = Tensor2D<M, O, Mgr>;
    fn mul(self, rhs: &Tensor2D<N, O, NoTape>) -> Self::Output {
        matmat_mul(self, rhs)
    }
}

pub fn vecmat_mul<const N: usize, const O: usize, Mgr: TapeManager>(
    lhs: Tensor1D<N, Mgr>,
    rhs: &Tensor2D<N, O, NoTape>,
) -> Tensor1D<O, Mgr> {
    let result = Tensor1D::new(lhs.data().dot(rhs.data()));
    let (lhs, mut tape_manager) = lhs.split_tape_manager();
    tape_manager.update_with(|tape| {
        binary_op(
            tape,
            OpType::MatMul { m: 1, n: N, o: O },
            (&lhs, rhs, &result),
            // NOTE: the derivatives here are reversed for matrix multiplication
            rhs.data.clone(),
            lhs.data.clone(),
        )
    });
    result.with_tape_manager(tape_manager)
}

impl<const N: usize, const O: usize, Mgr: TapeManager> std::ops::Mul<&Tensor2D<N, O, NoTape>>
    for Tensor1D<N, Mgr>
{
    type Output = Tensor1D<O, Mgr>;
    fn mul(self, rhs: &Tensor2D<N, O, NoTape>) -> Self::Output {
        vecmat_mul(self, rhs)
    }
}

pub fn broadcast_add<const M: usize, const N: usize, Mgr: TapeManager>(
    lhs: Tensor2D<M, N, Mgr>,
    rhs: &Tensor1D<N, NoTape>,
) -> Tensor2D<M, N, Mgr> {
    let result = Tensor2D::new(lhs.data() + rhs.data());
    let (lhs, mut tape_manager) = lhs.split_tape_manager();
    tape_manager.update_with(|tape| {
        binary_op(
            tape,
            OpType::Broadcast,
            (&lhs, rhs, &result),
            Array::from_elem(lhs.shape(), 1.0),
            Array::from_elem(rhs.shape(), 1.0 / M as f32),
        )
    });
    result.with_tape_manager(tape_manager)
}

impl<const M: usize, const N: usize, Mgr: TapeManager> std::ops::Add<&Tensor1D<N, NoTape>>
    for Tensor2D<M, N, Mgr>
{
    type Output = Tensor2D<M, N, Mgr>;
    fn add(self, rhs: &Tensor1D<N, NoTape>) -> Self::Output {
        broadcast_add(self, rhs)
    }
}

pub fn add<T: Tensor>(lhs: &T::NoTape, rhs: T) -> T
where
    T::NoTape: TensorCreator + CanAddTape<T::TapeManager, Output = T>,
{
    let result = T::NoTape::new(lhs.data() + rhs.data());
    let (rhs, mut tape_manager) = rhs.split_tape_manager();
    tape_manager.update_with(|tape| {
        binary_op(
            tape,
            OpType::Normal,
            (lhs, &rhs, &result),
            Array::from_elem(lhs.shape(), 1.0),
            Array::from_elem(rhs.shape(), 1.0),
        );
    });
    result.with_tape_manager(tape_manager)
}

pub fn sub<T: Tensor>(lhs: &T::NoTape, rhs: T) -> T
where
    T::NoTape: TensorCreator + CanAddTape<T::TapeManager, Output = T>,
{
    let result = T::NoTape::new(lhs.data() - rhs.data());
    let (rhs, mut tape_manager) = rhs.split_tape_manager();
    tape_manager.update_with(|tape| {
        binary_op(
            tape,
            OpType::Normal,
            (lhs, &rhs, &result),
            Array::from_elem(lhs.shape(), 1.0),
            Array::from_elem(rhs.shape(), -1.0),
        );
    });
    result.with_tape_manager(tape_manager)
}

pub fn mul<T: Tensor>(lhs: &T::NoTape, rhs: T) -> T
where
    T::NoTape: TensorCreator + CanAddTape<T::TapeManager, Output = T>,
{
    let result = T::NoTape::new(lhs.data() * rhs.data());
    let (rhs, mut tape_manager) = rhs.split_tape_manager();
    tape_manager.update_with(|tape| {
        binary_op(
            tape,
            OpType::Normal,
            (lhs, &rhs, &result),
            rhs.data().clone(),
            lhs.data().clone(),
        );
    });
    result.with_tape_manager(tape_manager)
}

macro_rules! binary_ops_impl {
    ($typename:ident, [$($const_names:tt),*]) => {

// &T<NoTape> + T<Mgr>
impl<$(const $const_names: usize, )* Mgr: TapeManager> std::ops::Add<$typename<$($const_names, )* Mgr>> for &$typename<$($const_names, )* NoTape> {
    type Output = $typename<$($const_names, )* Mgr>;
    fn add(self, rhs: $typename<$($const_names, )* Mgr>) -> Self::Output {
        add(self, rhs)
    }
}

// &T<NoTape> - T<Mgr>
impl<$(const $const_names: usize, )* Mgr: TapeManager> std::ops::Sub<$typename<$($const_names, )* Mgr>> for &$typename<$($const_names, )* NoTape> {
    type Output = $typename<$($const_names, )* Mgr>;
    fn sub(self, rhs: $typename<$($const_names, )* Mgr>) -> Self::Output {
        sub(self, rhs)
    }
}

// &T<NoTape> * T<Mgr>
impl<$(const $const_names: usize, )* Mgr: TapeManager> std::ops::Mul<$typename<$($const_names, )* Mgr>> for &$typename<$($const_names, )* NoTape> {
    type Output = $typename<$($const_names, )* Mgr>;
    fn mul(self, rhs: $typename<$($const_names, )* Mgr>) -> Self::Output {
        mul(self, rhs)
    }
}
    };
}

binary_ops_impl!(Tensor0D, []);
binary_ops_impl!(Tensor1D, [N]);
binary_ops_impl!(Tensor2D, [M, N]);
binary_ops_impl!(Tensor3D, [M, N, O]);
binary_ops_impl!(Tensor4D, [M, N, O, P]);
