use alloy::primitives::{U256, U512};
use alloy::uint;

pub const SCALE: U256 = uint!(1_000_000_U256);

#[inline]
fn u512_to_u256(q: U512) -> U256 {
    let limbs = q.into_limbs();
    let hi = &limbs[4..];
    assert!(hi.iter().all(|&w| w == 0), "overflow to U256");
    U256::from_limbs([limbs[0], limbs[1], limbs[2], limbs[3]])
}

#[inline]
pub fn mul_div(a: U256, b: U256, d: U256) -> U256 {
    let q: U512 = (U512::from(a) * U512::from(b)) / U512::from(d);

    u512_to_u256(q)
}

pub fn optimal_split(pools: &[(U256, U256)], amount_in: U256) -> Vec<U256> {
    let ai = |x: U256, y: U256, lambda: U256| -> U256 {
        if lambda.is_zero() {
            return U256::ZERO;
        }

        let k = mul_div(y * U256::from(u128::MAX), x, lambda);

        let s = k.root(2);
        if s <= x {
            return U256::ZERO;
        }

        s - x
    };

    let mut lo = U256::ZERO;
    let mut hi = U256::MAX >> 1;

    for _ in 0..256 {
        let mid = (lo + hi) >> 1;
        let total: U256 = pools.iter().map(|&(x, y)| ai(x, y, mid)).sum();
        if total > amount_in {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    let lambda = hi;

    let mut split: Vec<U256> = pools.iter().map(|&(x, y)| ai(x, y, lambda)).collect();

    let allocated: U256 = split.iter().copied().sum();
    if allocated < amount_in {
        let rem = amount_in - allocated;
        let index = split
            .iter()
            .enumerate()
            .max_by_key(|&(_, split)| split)
            .map(|(index, _)| index)
            .unwrap_or(0);
        split[index] += rem;
    } else if allocated > amount_in {
        let mut i = 0;
        let mut rem = allocated - amount_in;
        while !rem.is_zero() {
            if split[i] > U256::ZERO {
                split[i] -= U256::ONE;
                rem -= U256::ONE;
            }
            i = (i + 1) % split.len();
        }
    }

    debug_assert_eq!(split.iter().copied().sum::<U256>(), amount_in);
    split
}

pub fn amount_out(amount_in: U256, reserve_in: U256, reserve_out: U256, fee_ppm: u32) -> U256 {
    assert!(fee_ppm < 1_000_000, "fee must be < 100 %");
    if amount_in.is_zero() || reserve_in.is_zero() || reserve_out.is_zero() {
        return U256::ZERO;
    }

    let r_num = SCALE - U256::from(fee_ppm);
    let a_in_fee = mul_div(amount_in, r_num, SCALE);

    let numerator = a_in_fee * reserve_out;
    let denominator = reserve_in + a_in_fee;

    if denominator.is_zero() {
        return U256::ZERO;
    }

    numerator / denominator
}

pub fn total_output(pools: &[(U256, U256)], split: &[U256], fee_ppm: u32) -> U256 {
    assert_eq!(pools.len(), split.len(), "pools vs split mismatch");
    pools
        .iter()
        .zip(split)
        .map(|(&(x, y), &a)| amount_out(a, x, y, fee_ppm))
        .sum()
}
