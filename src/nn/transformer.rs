use crate::prelude::*;
use rand::Rng;

/// A multi-head attention layer.
///
/// # Generics
/// - `M` The embedding size of token vectors from decoder.
/// - `N` The embedding size of token vectors from encoder.
/// - `K` The size of the keys in self attention.
/// - `V` The size of the values.
/// - `H` The number of attention heads.
///
/// # Examples
/// `MultiHeadAttention<8, 10, 10, 10, 2>` is an attention layer with 2 heads and 10 token, key and value dims.
/// TODO: Doctests fail for some reason
#[derive(Debug, Clone, Default)]
pub struct MultiHeadAttention<
    const M: usize,
    const N: usize,
    const K: usize,
    const V: usize,
    const H: usize,
> {
    w_q: Linear<N, K>,
    w_k: Linear<M, K>,
    w_v: Linear<M, V>,
    w_o: Linear<V, M>,
}

impl<const M: usize, const N: usize, const K: usize, const V: usize, const H: usize> ResetParams
    for MultiHeadAttention<M, N, K, V, H>
{
    fn reset_params<R: Rng>(&mut self, rng: &mut R) {
        self.w_q.reset_params(rng);
        self.w_k.reset_params(rng);
        self.w_v.reset_params(rng);
        self.w_o.reset_params(rng);
    }
}

impl<const M: usize, const N: usize, const K: usize, const V: usize, const H: usize>
    CanUpdateWithGradients for MultiHeadAttention<M, N, K, V, H>
{
    fn update<G: GradientProvider>(&mut self, grads: &mut G) {
        self.w_q.update(grads);
        self.w_k.update(grads);
        self.w_v.update(grads);
        self.w_o.update(grads);
    }
}

/// Normal self attention (where same tensors are used for keys, queries and values)
impl<const M: usize, const K: usize, const V: usize, const S: usize, const H: usize>
    Module<Tensor2D<S, M>> for MultiHeadAttention<M, M, K, V, H>
where
    Assert<{ S * K == H * S * (K / H) }>: ConstTrue,
    Assert<{ S * V == H * S * (V / H) }>: ConstTrue,
    Assert<{ H * S * (V / H) == S * V }>: ConstTrue,
{
    type Output = Tensor2D<S, M>;

    fn forward(&self, input: Tensor2D<S, M>) -> Self::Output {
        let queries = self.w_q.forward(input.duplicate());
        let keys = self.w_k.forward(input.duplicate());
        let values = self.w_v.forward(input);

        let keys: Tensor3D<H, S, { K / H }> = keys.reshape();
        let queries: Tensor3D<H, S, { K / H }> = queries.reshape();
        let values: Tensor3D<H, S, { V / H }> = values.reshape();

        // Get weights
        let token_weights = batch_3d_matmul_transpose(queries, &keys) / (M as f32);

        // Softmax on last dimension
        let token_weights = softmax(token_weights);

        // Get new tokens
        let tokens: Tensor3D<H, S, { V / H }> = batch_3d_matmul(token_weights, &values);
        let tokens: Tensor2D<S, V> = tokens.reshape();
        self.w_o.forward(tokens)
    }
}

/// Encoder-Decoder style self attention where one set of tensors is used for values and keys, and another is used for queries
impl<
        const M: usize,
        const N: usize,
        const K: usize,
        const V: usize,
        const S1: usize,
        const S2: usize,
        const H: usize,
    > Module<(Tensor2D<S1, M>, Tensor2D<S2, N>)> for MultiHeadAttention<M, N, K, V, H>
where
    Assert<{ S2 * K == H * S2 * (K / H) }>: ConstTrue,
    Assert<{ S1 * K == H * S1 * (K / H) }>: ConstTrue,
    Assert<{ S1 * V == H * S1 * (V / H) }>: ConstTrue,
    Assert<{ S2 * H * { V / H } == S2 * V }>: ConstTrue,
    Assert<{ H * S2 * (V / H) == S2 * V }>: ConstTrue,
{
    type Output = Tensor2D<S2, M>;

    fn forward(&self, (from_enc, input): (Tensor2D<S1, M>, Tensor2D<S2, N>)) -> Self::Output {
        let queries = self.w_q.forward(input);
        let keys = self.w_k.forward(from_enc.duplicate());
        let values = self.w_v.forward(from_enc);

        let keys: Tensor3D<H, S1, { K / H }> = keys.reshape();
        let queries: Tensor3D<H, S2, { K / H }> = queries.reshape();
        let values: Tensor3D<H, S1, { V / H }> = values.reshape();

        // Get weights
        let token_weights = batch_3d_matmul_transpose(queries, &keys) / (M as f32);

        // Softmax on last dimension
        let token_weights = softmax(token_weights);

        // Get new tokens
        let tokens: Tensor2D<S2, V> = batch_3d_matmul(token_weights, &values).reshape();
        self.w_o.forward(tokens)
    }
}

/// Batched normal self attention (where same tensors are used for keys, queries and values)
impl<const B: usize, const M: usize, const K: usize, const V: usize, const S: usize, const H: usize>
    Module<Tensor3D<B, S, M>> for MultiHeadAttention<M, M, K, V, H>
where
    Assert<{ B * S * K == B * H * S * (K / H) }>: ConstTrue,
    Assert<{ B * S * V == B * H * S * (V / H) }>: ConstTrue,
    Assert<{ B * H * S * (V / H) == B * S * V }>: ConstTrue,
{
    type Output = Tensor3D<B, S, M>;

    fn forward(&self, input: Tensor3D<B, S, M>) -> Self::Output {
        let queries = self.w_q.forward(input.duplicate());
        let keys = self.w_k.forward(input.duplicate());
        let values = self.w_v.forward(input);

        let keys: Tensor4D<B, H, S, { K / H }> = keys.reshape();
        let queries: Tensor4D<B, H, S, { K / H }> = queries.reshape();
        let values: Tensor4D<B, H, S, { V / H }> = values.reshape();

        // Get weights
        let token_weights = batch_4d_matmul_transpose(queries, &keys) / (M as f32);

        // Softmax on last dimension
        let token_weights = softmax(token_weights);

        // Get new tokens
        let tokens: Tensor4D<B, H, S, { V / H }> = batch_4d_matmul(token_weights, &values);
        let tokens: Tensor3D<B, S, V> = tokens.reshape();
        self.w_o.forward(tokens)
    }
}

/// Batched Encoder-Decoder style self attention where one set of tensors is used for values and keys, and another is used for queries
impl<
        const B: usize,
        const M: usize,
        const N: usize,
        const K: usize,
        const V: usize,
        const S1: usize,
        const S2: usize,
        const H: usize,
    > Module<(Tensor3D<B, S1, M>, Tensor3D<B, S2, N>)> for MultiHeadAttention<M, N, K, V, H>
where
    Assert<{ B * S2 * K == B * H * S2 * (K / H) }>: ConstTrue,
    Assert<{ B * S1 * K == B * H * S1 * (K / H) }>: ConstTrue,
    Assert<{ B * S1 * V == B * H * S1 * (V / H) }>: ConstTrue,
    Assert<{ B * S2 * H * { V / H } == B * S2 * V }>: ConstTrue,
    Assert<{ B * H * S2 * (V / H) == B * S2 * V }>: ConstTrue,
{
    type Output = Tensor3D<B, S2, M>;

    fn forward(&self, (from_enc, input): (Tensor3D<B, S1, M>, Tensor3D<B, S2, N>)) -> Self::Output {
        let queries = self.w_q.forward(input);
        let keys = self.w_k.forward(from_enc.duplicate());
        let values = self.w_v.forward(from_enc);

        let keys: Tensor4D<B, H, S1, { K / H }> = keys.reshape();
        let queries: Tensor4D<B, H, S2, { K / H }> = queries.reshape();
        let values: Tensor4D<B, H, S1, { V / H }> = values.reshape();

        // Get weights
        let token_weights = batch_4d_matmul_transpose(queries, &keys) / (M as f32);

        // Softmax on last dimension
        let token_weights = softmax(token_weights);

        // Get new tokens
        let tokens: Tensor3D<B, S2, V> = batch_4d_matmul(token_weights, &values).reshape();
        self.w_o.forward(tokens)
    }
}

/// A single transformer block containing self attention, feed forward and layer norms
#[derive(Debug, Clone, Default)]
pub struct TransformerBlock<const M: usize, const I: usize, const K: usize, const H: usize> {
    attn: MultiHeadAttention<M, M, K, M, H>,
    norm1: LayerNorm1D<M>,
    norm2: LayerNorm1D<M>,
    ff: (Linear<M, I>, ReLU, Linear<I, M>),
}

impl<const M: usize, const I: usize, const K: usize, const H: usize> ResetParams
    for TransformerBlock<M, I, K, H>
{
    fn reset_params<R: Rng>(&mut self, rng: &mut R) {
        self.attn.reset_params(rng);
        self.norm1.reset_params(rng);
        self.norm2.reset_params(rng);
        self.ff.reset_params(rng);
    }
}

impl<const M: usize, const I: usize, const K: usize, const H: usize> CanUpdateWithGradients
    for TransformerBlock<M, I, K, H>
{
    fn update<G: GradientProvider>(&mut self, grads: &mut G) {
        self.attn.update(grads);
        self.norm1.update(grads);
        self.norm2.update(grads);
        self.ff.update(grads);
    }
}

/// Single sequence impl
impl<const M: usize, const I: usize, const K: usize, const S: usize, const H: usize>
    Module<Tensor2D<S, M>> for TransformerBlock<M, I, K, H>
where
    Assert<{ H * S * (M / H) == S * M }>: ConstTrue,
    Assert<{ S * K == H * S * (K / H) }>: ConstTrue,
    Assert<{ S * M == H * S * (M / H) }>: ConstTrue,
{
    type Output = Tensor2D<S, M>;

    fn forward(&self, input: Tensor2D<S, M>) -> Self::Output {
        let x = self
            .norm1
            .forward(input.duplicate() + &self.attn.forward(input));
        self.norm2.forward(x.duplicate() + &self.ff.forward(x))
    }
}

/// Batch sequence impl
impl<const M: usize, const I: usize, const K: usize, const S: usize, const B: usize, const H: usize> Module<Tensor3D<B, S, M>> for TransformerBlock<M, I, K, H> 
where Assert<{B * H * S * (M / H) == B * S * M }>: ConstTrue,
    Assert<{B * S * K == B * H * S * (K / H) }>: ConstTrue,
    Assert<{B * S * M == B * H * S * (M / H) }>: ConstTrue {
    type Output = Tensor3D<B, S, M>;

    fn forward(&self, input: Tensor3D<B, S, M>) -> Self::Output {
        let x = self.norm1.forward(input.duplicate() + &self.attn.forward(input));
        self.norm2.forward(x.duplicate() + &self.ff.forward(x))
    }
}

/// A transformer encoder.
///
/// # Generics
/// - `M` The embedding size of token vectors.
/// - `I` The inner size of the feedforward layers.
/// - `L` The number of layers.
/// - `H` The number of heads for self attention.
/// TODO: Doctests
#[derive(Debug, Clone)]
pub struct TransformerEncoder<const M: usize, const I: usize, const L: usize, const H: usize>
where
    Assert<{ M % H == 0 }>: ConstTrue,
{
    blocks: Repeated<TransformerBlock<M, I, M, H>, L>,
}

impl<const M: usize, const I: usize, const L: usize, const H: usize> Default
    for TransformerEncoder<M, I, L, H>
where
    Assert<{ M % H == 0 }>: ConstTrue,
    [TransformerBlock<M, I, M, H>; L]: Default,
{
    fn default() -> Self {
        Self {
            blocks: Default::default(),
        }
    }
}

impl<const M: usize, const I: usize, const L: usize, const H: usize> ResetParams
    for TransformerEncoder<M, I, L, H>
where
    Assert<{ M % H == 0 }>: ConstTrue,
{
    fn reset_params<R: Rng>(&mut self, rng: &mut R) {
        self.blocks.reset_params(rng);
    }
}

impl<const M: usize, const I: usize, const L: usize, const H: usize> CanUpdateWithGradients
    for TransformerEncoder<M, I, L, H>
where
    Assert<{ M % H == 0 }>: ConstTrue,
{
    fn update<G: GradientProvider>(&mut self, grads: &mut G) {
        self.blocks.update(grads);
    }
}

impl<const S: usize, const M: usize, const I: usize, const L: usize, const H: usize>
    Module<Tensor2D<S, M>> for TransformerEncoder<M, I, L, H>
where
    Assert<{ M % H == 0 }>: ConstTrue,
    Assert<{ S * M == S * H * (M / H) }>: ConstTrue,
    Assert<{ H * S * (M / H) == S * M }>: ConstTrue,
    Assert<{ S * M == H * S * (M / H) }>: ConstTrue,
{
    type Output = Tensor2D<S, M>;

    fn forward(&self, input: Tensor2D<S, M>) -> Self::Output {
        self.blocks.forward(input)
    }
}

impl<const B: usize, const S: usize, const M: usize, const I: usize, const H: usize, const L: usize>
    Module<Tensor3D<B, S, M>> for TransformerEncoder<M, I, L, H>
where Assert<{ M % H == 0 }>: ConstTrue,
    Assert<{ B * S * M == B * S * H * (M / H) }>: ConstTrue,
    Assert<{ B * H * S * (M / H) == B * S * M }>: ConstTrue,
    Assert<{ B * S * M == B * H * S * (M / H) }>: ConstTrue,
{
    type Output = Tensor3D<B, S, M>;

    fn forward(&self, input: Tensor3D<B, S, M>) -> Self::Output {
        self.blocks.forward(input)
    }
}

#[cfg(test)]
mod tests {
    use rand::thread_rng;

    use crate::tests::assert_close;

    use super::*;

    #[test]
    fn test_self_attention() {
        let model: MultiHeadAttention<8, 8, 8, 8, 1> = MultiHeadAttention {
            w_q: Linear {
                weight: Tensor2D::new([
                    [
                        0.1574, -0.2003, 0.0850, 0.2589, -0.0813, 0.0932, -0.0137, 0.2020,
                    ],
                    [
                        0.2021, -0.1780, -0.2722, -0.1615, 0.4079, -0.3185, -0.3676, -0.2339,
                    ],
                    [
                        -0.4066, 0.4068, -0.0236, 0.2187, 0.0192, -0.2541, -0.3628, -0.3462,
                    ],
                    [
                        -0.3576, 0.1455, -0.2628, -0.3512, 0.2617, 0.4011, -0.1893, 0.0074,
                    ],
                    [
                        0.3362, -0.1857, -0.1462, 0.2258, 0.2525, -0.1959, 0.4204, 0.0527,
                    ],
                    [
                        -0.2779, 0.2277, 0.0287, -0.3090, -0.2154, -0.3343, -0.4102, 0.1247,
                    ],
                    [
                        0.1978, -0.0637, 0.3727, -0.1929, -0.2977, 0.0057, 0.2015, -0.3023,
                    ],
                    [
                        -0.0626, -0.3986, -0.0338, 0.0366, -0.3096, 0.1367, -0.0734, -0.3320,
                    ],
                ]),
                bias: Tensor1D::new([
                    0.0773, -0.2218, 0.0269, 0.2612, 0.2109, -0.2013, 0.0431, -0.0836,
                ]),
            },
            w_k: Linear {
                weight: Tensor2D::new([
                    [
                        0.2069, 0.0154, -0.2676, -0.3061, -0.2987, -0.3143, -0.3604, 0.1183,
                    ],
                    [
                        -0.4073, -0.4290, 0.1581, -0.0480, -0.0837, 0.2044, -0.0503, -0.3374,
                    ],
                    [
                        -0.3744, 0.1356, 0.3755, -0.4040, -0.3553, 0.2100, -0.2551, 0.0068,
                    ],
                    [
                        0.0492, -0.0793, 0.3224, 0.0782, 0.4010, -0.2416, 0.2916, -0.1970,
                    ],
                    [
                        0.2141, -0.4310, 0.1829, 0.1139, -0.1791, -0.0331, -0.2026, -0.0118,
                    ],
                    [
                        0.0752, 0.2486, 0.3596, 0.2715, 0.1719, 0.3920, 0.1833, -0.0486,
                    ],
                    [
                        -0.3989, -0.4021, -0.1274, -0.1533, -0.2212, 0.2649, -0.0964, 0.0363,
                    ],
                    [
                        -0.2067, 0.1342, 0.4172, -0.1923, 0.3606, 0.1490, -0.1655, -0.2564,
                    ],
                ]),
                bias: Tensor1D::new([
                    0.3314, 0.1901, -0.2715, 0.1083, 0.0523, 0.2471, 0.3526, -0.3369,
                ]),
            },
            w_v: Linear {
                weight: Tensor2D::new([
                    [
                        0.2284, -0.1289, 0.0660, 0.3557, 0.0571, -0.1956, 0.3716, -0.3293,
                    ],
                    [
                        0.0483, 0.1731, 0.2582, 0.1026, -0.1180, -0.0721, -0.1970, -0.3602,
                    ],
                    [
                        -0.1556, 0.0342, 0.2193, -0.2418, 0.2231, -0.0216, 0.0725, -0.2824,
                    ],
                    [
                        -0.1965, -0.0953, -0.2434, 0.1300, -0.3424, 0.2907, -0.1313, 0.3331,
                    ],
                    [
                        0.0551, 0.1247, 0.2200, 0.0062, -0.4232, -0.1389, 0.1476, -0.0718,
                    ],
                    [
                        -0.0776, -0.3066, -0.0368, -0.1757, -0.0697, -0.2670, 0.1791, 0.2097,
                    ],
                    [
                        -0.0299, 0.3960, 0.1764, 0.0571, 0.2683, -0.3625, 0.2716, -0.1853,
                    ],
                    [
                        -0.3581, -0.1497, -0.2204, -0.1340, -0.0511, 0.2451, -0.1244, 0.1805,
                    ],
                ]),
                bias: Tensor1D::new([
                    -0.0994, -0.1629, -0.2694, -0.0869, 0.1631, -0.1892, -0.0901, 0.3148,
                ]),
            },
            w_o: Linear {
                weight: Tensor2D::new([
                    [
                        0.1190, -0.2099, 0.1869, -0.3508, 0.0826, -0.3263, 0.2366, -0.2100,
                    ],
                    [
                        0.2002, -0.2365, -0.1015, 0.2539, -0.1125, 0.2926, -0.0981, -0.1495,
                    ],
                    [
                        -0.1831, 0.0348, 0.2623, 0.1650, 0.2114, -0.0376, 0.1850, -0.3326,
                    ],
                    [
                        -0.0636, 0.1737, -0.1024, -0.0246, 0.2178, -0.3127, -0.0506, 0.0568,
                    ],
                    [
                        -0.3384, -0.1202, -0.2316, 0.0117, 0.2929, -0.2060, 0.1966, -0.3274,
                    ],
                    [
                        0.2589, 0.3003, -0.2277, 0.2488, -0.0594, -0.0645, 0.0931, 0.2376,
                    ],
                    [
                        0.3371, 0.0463, -0.1292, 0.1341, 0.2008, -0.0325, 0.0914, 0.0517,
                    ],
                    [
                        -0.2241, 0.0426, 0.2326, -0.3048, -0.2760, -0.0868, -0.2429, 0.1446,
                    ],
                ]),
                bias: Tensor1D::new([
                    0.0800, 0.0567, 0.2609, -0.1651, -0.0820, -0.1058, -0.3133, -0.1181,
                ]),
            },
        };
        let x: Tensor2D<2, 8> = Tensor2D::new([
            [
                0.7207, 0.3572, 0.2341, 0.4865, 0.2949, 0.5450, 0.8236, 0.4674,
            ],
            [
                0.4800, 0.6774, 0.9052, 0.4714, 0.5683, 0.7339, 0.1975, 0.3909,
            ],
        ]); // Sequence of 2 token vectors with 8 dims each
        let y: Tensor2D<2, 8> = model.forward(x);
        assert_close(
            y.data(),
            &[
                [
                    0.3727123,
                    -0.06548928,
                    0.1931893,
                    0.015571773,
                    0.16583481,
                    -0.073905066,
                    -0.19729866,
                    -0.21708608,
                ],
                [
                    0.37352866,
                    -0.067618236,
                    0.19414166,
                    0.016556486,
                    0.16638368,
                    -0.074163824,
                    -0.19822657,
                    -0.21566412,
                ],
            ],
        );
    }

    #[test]
    fn test_matmul() {
        let mut rng = thread_rng();
        let inp: Tensor1D<4> = Tensor1D::rand(&mut rng);
        let out: Tensor2D<2, 2> = inp.duplicate().reshape();
        println!("Inp: {:?}", inp);
        println!("Out: {:?}", out);
        //panic!("")
    }

    #[test]
    fn test_transformer_encoder() {
        let model: TransformerEncoder<8, 16, 1, 2> = TransformerEncoder {
            blocks: Repeated {
                modules: [TransformerBlock {
                    attn: MultiHeadAttention {
                        w_q: Linear {
                            weight: Tensor2D::new([
                                [
                                    0.1574, -0.2003, 0.0850, 0.2589, -0.0813, 0.0932, -0.0137,
                                    0.2020,
                                ],
                                [
                                    0.2021, -0.1780, -0.2722, -0.1615, 0.4079, -0.3185, -0.3676,
                                    -0.2339,
                                ],
                                [
                                    -0.4066, 0.4068, -0.0236, 0.2187, 0.0192, -0.2541, -0.3628,
                                    -0.3462,
                                ],
                                [
                                    -0.3576, 0.1455, -0.2628, -0.3512, 0.2617, 0.4011, -0.1893,
                                    0.0074,
                                ],
                                [
                                    0.3362, -0.1857, -0.1462, 0.2258, 0.2525, -0.1959, 0.4204,
                                    0.0527,
                                ],
                                [
                                    -0.2779, 0.2277, 0.0287, -0.3090, -0.2154, -0.3343, -0.4102,
                                    0.1247,
                                ],
                                [
                                    0.1978, -0.0637, 0.3727, -0.1929, -0.2977, 0.0057, 0.2015,
                                    -0.3023,
                                ],
                                [
                                    -0.0626, -0.3986, -0.0338, 0.0366, -0.3096, 0.1367, -0.0734,
                                    -0.3320,
                                ],
                            ]),
                            bias: Tensor1D::new([
                                0.0773, -0.2218, 0.0269, 0.2612, 0.2109, -0.2013, 0.0431, -0.0836,
                            ]),
                        },
                        w_k: Linear {
                            weight: Tensor2D::new([
                                [
                                    0.2069, 0.0154, -0.2676, -0.3061, -0.2987, -0.3143, -0.3604,
                                    0.1183,
                                ],
                                [
                                    -0.4073, -0.4290, 0.1581, -0.0480, -0.0837, 0.2044, -0.0503,
                                    -0.3374,
                                ],
                                [
                                    -0.3744, 0.1356, 0.3755, -0.4040, -0.3553, 0.2100, -0.2551,
                                    0.0068,
                                ],
                                [
                                    0.0492, -0.0793, 0.3224, 0.0782, 0.4010, -0.2416, 0.2916,
                                    -0.1970,
                                ],
                                [
                                    0.2141, -0.4310, 0.1829, 0.1139, -0.1791, -0.0331, -0.2026,
                                    -0.0118,
                                ],
                                [
                                    0.0752, 0.2486, 0.3596, 0.2715, 0.1719, 0.3920, 0.1833, -0.0486,
                                ],
                                [
                                    -0.3989, -0.4021, -0.1274, -0.1533, -0.2212, 0.2649, -0.0964,
                                    0.0363,
                                ],
                                [
                                    -0.2067, 0.1342, 0.4172, -0.1923, 0.3606, 0.1490, -0.1655,
                                    -0.2564,
                                ],
                            ]),
                            bias: Tensor1D::new([
                                0.3314, 0.1901, -0.2715, 0.1083, 0.0523, 0.2471, 0.3526, -0.3369,
                            ]),
                        },
                        w_v: Linear {
                            weight: Tensor2D::new([
                                [
                                    0.2284, -0.1289, 0.0660, 0.3557, 0.0571, -0.1956, 0.3716,
                                    -0.3293,
                                ],
                                [
                                    0.0483, 0.1731, 0.2582, 0.1026, -0.1180, -0.0721, -0.1970,
                                    -0.3602,
                                ],
                                [
                                    -0.1556, 0.0342, 0.2193, -0.2418, 0.2231, -0.0216, 0.0725,
                                    -0.2824,
                                ],
                                [
                                    -0.1965, -0.0953, -0.2434, 0.1300, -0.3424, 0.2907, -0.1313,
                                    0.3331,
                                ],
                                [
                                    0.0551, 0.1247, 0.2200, 0.0062, -0.4232, -0.1389, 0.1476,
                                    -0.0718,
                                ],
                                [
                                    -0.0776, -0.3066, -0.0368, -0.1757, -0.0697, -0.2670, 0.1791,
                                    0.2097,
                                ],
                                [
                                    -0.0299, 0.3960, 0.1764, 0.0571, 0.2683, -0.3625, 0.2716,
                                    -0.1853,
                                ],
                                [
                                    -0.3581, -0.1497, -0.2204, -0.1340, -0.0511, 0.2451, -0.1244,
                                    0.1805,
                                ],
                            ]),
                            bias: Tensor1D::new([
                                -0.0994, -0.1629, -0.2694, -0.0869, 0.1631, -0.1892, -0.0901,
                                0.3148,
                            ]),
                        },
                        w_o: Linear {
                            weight: Tensor2D::new([
                                [
                                    0.1190, -0.2099, 0.1869, -0.3508, 0.0826, -0.3263, 0.2366,
                                    -0.2100,
                                ],
                                [
                                    0.2002, -0.2365, -0.1015, 0.2539, -0.1125, 0.2926, -0.0981,
                                    -0.1495,
                                ],
                                [
                                    -0.1831, 0.0348, 0.2623, 0.1650, 0.2114, -0.0376, 0.1850,
                                    -0.3326,
                                ],
                                [
                                    -0.0636, 0.1737, -0.1024, -0.0246, 0.2178, -0.3127, -0.0506,
                                    0.0568,
                                ],
                                [
                                    -0.3384, -0.1202, -0.2316, 0.0117, 0.2929, -0.2060, 0.1966,
                                    -0.3274,
                                ],
                                [
                                    0.2589, 0.3003, -0.2277, 0.2488, -0.0594, -0.0645, 0.0931,
                                    0.2376,
                                ],
                                [
                                    0.3371, 0.0463, -0.1292, 0.1341, 0.2008, -0.0325, 0.0914,
                                    0.0517,
                                ],
                                [
                                    -0.2241, 0.0426, 0.2326, -0.3048, -0.2760, -0.0868, -0.2429,
                                    0.1446,
                                ],
                            ]),
                            bias: Tensor1D::new([
                                0.0800, 0.0567, 0.2609, -0.1651, -0.0820, -0.1058, -0.3133, -0.1181,
                            ]),
                        },
                    },
                    norm1: LayerNorm1D {
                        gamma: Tensor1D::new([1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0]),
                        beta: Tensor1D::new([0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0]),
                        epsilon: 1e-5,
                    },
                    norm2: LayerNorm1D {
                        gamma: Tensor1D::new([1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0]),
                        beta: Tensor1D::new([0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0]),
                        epsilon: 1e-5,
                    },
                    ff: (
                        Linear {
                            weight: Tensor2D::new([
                                [
                                    0.32741955,
                                    -0.27696532,
                                    -0.29539806,
                                    0.0069717765,
                                    -0.31420162,
                                    -0.036986142,
                                    -0.3338193,
                                    0.17511156,
                                ],
                                [
                                    0.13717508,
                                    0.18659177,
                                    -0.33520392,
                                    -0.17715862,
                                    -0.07881671,
                                    -0.018760145,
                                    0.0051377714,
                                    -0.072549134,
                                ],
                                [
                                    0.14625782,
                                    -0.07704654,
                                    -0.29890594,
                                    -0.26514953,
                                    -0.22648443,
                                    0.031164229,
                                    0.019266665,
                                    0.1848819,
                                ],
                                [
                                    -0.1651381,
                                    0.34296212,
                                    0.32781574,
                                    0.013631046,
                                    -0.3226381,
                                    -0.044470996,
                                    0.11201647,
                                    -0.18100157,
                                ],
                                [
                                    -0.05857131,
                                    -0.25194114,
                                    0.1025781,
                                    -0.0677101,
                                    -0.2702725,
                                    0.02451101,
                                    -0.07078883,
                                    0.11940616,
                                ],
                                [
                                    0.06682053,
                                    -0.17431287,
                                    0.019746482,
                                    0.053622603,
                                    -0.069140226,
                                    0.13807291,
                                    -0.24376759,
                                    -0.33779323,
                                ],
                                [
                                    0.29472074,
                                    -0.09042597,
                                    0.03473559,
                                    0.21112385,
                                    -0.14513023,
                                    0.10050717,
                                    -0.34709868,
                                    0.03904426,
                                ],
                                [
                                    -0.06940845,
                                    -0.054345757,
                                    0.12867263,
                                    -0.2869896,
                                    -0.2534054,
                                    0.068042785,
                                    0.35119823,
                                    -0.29233998,
                                ],
                                [
                                    -0.0871135,
                                    -0.16730882,
                                    0.13603503,
                                    0.15618584,
                                    -0.17793135,
                                    0.27447578,
                                    -0.21779232,
                                    0.18068978,
                                ],
                                [
                                    0.13410419,
                                    -0.28826278,
                                    -0.27560657,
                                    -0.31338045,
                                    -0.020344973,
                                    0.11338547,
                                    0.09660748,
                                    0.263692,
                                ],
                                [
                                    0.08829588,
                                    0.12593427,
                                    0.26640728,
                                    0.14729843,
                                    -0.22409765,
                                    -0.24616972,
                                    -0.14281164,
                                    0.12393728,
                                ],
                                [
                                    0.279121,
                                    0.2572312,
                                    0.120444745,
                                    -0.13078159,
                                    -0.03444299,
                                    -0.040969938,
                                    -0.14974514,
                                    0.2173976,
                                ],
                                [
                                    0.3032482,
                                    0.16804495,
                                    -0.25498143,
                                    0.2065703,
                                    0.26084676,
                                    0.18822041,
                                    -0.26633975,
                                    -0.18165638,
                                ],
                                [
                                    0.27341893,
                                    -0.28414428,
                                    0.27599248,
                                    0.25929502,
                                    -0.30611813,
                                    0.2899557,
                                    0.33385107,
                                    -0.26551148,
                                ],
                                [
                                    -0.21715835,
                                    0.041719317,
                                    -0.1648906,
                                    -0.06468901,
                                    -0.13153939,
                                    0.25872526,
                                    -0.14442022,
                                    0.27305982,
                                ],
                                [
                                    0.1998423,
                                    -0.15517804,
                                    -0.18107593,
                                    -0.22915882,
                                    -0.009488851,
                                    -0.021163374,
                                    -0.09172478,
                                    0.024704278,
                                ],
                            ]),
                            bias: Tensor1D::new([
                                0.3001593,
                                -0.12242858,
                                0.09446433,
                                0.34914228,
                                0.09442589,
                                -0.065933615,
                                0.08362213,
                                -0.27231246,
                                0.23779783,
                                0.34280863,
                                -0.03401932,
                                -0.18197946,
                                0.13024276,
                                0.0946002,
                                -0.14740522,
                                0.34740785,
                            ]),
                        },
                        ReLU::default(),
                        Linear {
                            weight: Tensor2D::new([
                                [
                                    0.001419425,
                                    0.18141323,
                                    0.10706228,
                                    -0.23785812,
                                    -0.035835326,
                                    0.16403425,
                                    0.0040647984,
                                    -0.15801013,
                                    0.1437453,
                                    -0.0059549212,
                                    0.110206544,
                                    0.038086593,
                                    0.22769731,
                                    0.1679703,
                                    -0.11711705,
                                    0.091575325,
                                ],
                                [
                                    -0.090115786,
                                    -0.18512738,
                                    -0.15485638,
                                    0.13144958,
                                    -0.09482396,
                                    -0.20131165,
                                    -0.046934605,
                                    -0.09465206,
                                    0.09745562,
                                    -0.1244055,
                                    0.16332954,
                                    0.18427557,
                                    0.028484464,
                                    -0.07190651,
                                    -0.032705367,
                                    0.23545647,
                                ],
                                [
                                    0.05309564,
                                    0.035057187,
                                    -0.21784127,
                                    0.24631232,
                                    -0.20846808,
                                    0.027608871,
                                    0.2208246,
                                    -0.021682918,
                                    0.12865263,
                                    0.08448297,
                                    0.04916519,
                                    -0.24629158,
                                    0.11192471,
                                    -0.16246903,
                                    -0.22969562,
                                    -0.20343202,
                                ],
                                [
                                    -0.008694649,
                                    0.19934464,
                                    0.088125885,
                                    -0.16100854,
                                    0.06579459,
                                    0.008887172,
                                    -0.15764749,
                                    -0.1632613,
                                    -0.024043322,
                                    0.20822823,
                                    -0.07804638,
                                    0.2184214,
                                    0.026557982,
                                    0.05165571,
                                    -0.18793088,
                                    0.24662256,
                                ],
                                [
                                    0.027863085,
                                    0.17160517,
                                    0.16212928,
                                    0.10069156,
                                    -0.075956285,
                                    -0.020970166,
                                    -0.20394576,
                                    0.15400726,
                                    0.08772743,
                                    0.069622934,
                                    -0.123850465,
                                    0.13160044,
                                    -0.06966215,
                                    0.009332895,
                                    -0.13149703,
                                    0.1316486,
                                ],
                                [
                                    0.12793612,
                                    0.096155584,
                                    0.13961542,
                                    0.17124552,
                                    -0.21669126,
                                    0.11740273,
                                    0.049543917,
                                    -0.13941693,
                                    0.09138793,
                                    -0.045862198,
                                    -0.003793776,
                                    -0.0824936,
                                    -0.091403365,
                                    -0.053946137,
                                    -0.14391446,
                                    -0.084465325,
                                ],
                                [
                                    -0.10627192,
                                    0.06024903,
                                    0.003990531,
                                    0.19410634,
                                    -0.02191186,
                                    -0.05462092,
                                    0.16804892,
                                    -0.06404108,
                                    -0.02612698,
                                    0.047793567,
                                    -0.18592936,
                                    -0.071727395,
                                    0.1410855,
                                    -0.0391829,
                                    -0.20302182,
                                    -0.052814245,
                                ],
                                [
                                    0.24361587,
                                    0.15750444,
                                    -0.04701662,
                                    -0.24509567,
                                    0.006577134,
                                    0.19346374,
                                    -0.06521648,
                                    0.08626419,
                                    0.24448293,
                                    -0.07718259,
                                    0.16315013,
                                    0.141895,
                                    0.06882566,
                                    0.058553517,
                                    -0.11471981,
                                    0.24982959,
                                ],
                            ]),
                            bias: Tensor1D::new([
                                -0.18349189,
                                -0.22159654,
                                0.053060174,
                                0.014924109,
                                -0.07278907,
                                -0.01328069,
                                0.118784845,
                                0.23260891,
                            ]),
                        },
                    ),
                }],
            },
        };
        let x: Tensor2D<2, 8> = Tensor2D::new([
            [
                0.2965, 0.7154, 0.9717, 0.5441, 0.7356, 0.2681, 0.4032, 0.4670,
            ],
            [
                0.7770, 0.1897, 0.0112, 0.6603, 0.6334, 0.4040, 0.1425, 0.1704,
            ],
        ]);
        let y: Tensor2D<2, 8> = model.forward(x);
        assert_close(
            y.data(),
            &[
                [
                    0.27625734,
                    0.38281897,
                    2.0543246,
                    -0.5007853,
                    0.4961129,
                    -1.3956192,
                    -1.010112,
                    -0.30299726,
                ],
                [
                    2.1115968,
                    -0.7773169,
                    -0.31452212,
                    0.5908173,
                    0.5295209,
                    -0.6455128,
                    -1.3281939,
                    -0.16638921,
                ],
            ],
        );
    }
}