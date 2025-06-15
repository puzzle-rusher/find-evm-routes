use alloy::primitives::U256;
use pool_algorithms::{mul_div, optimal_split, total_output};
use proptest::prelude::{ProptestConfig, prop};
use proptest::proptest;
use proptest::test_runner::FileFailurePersistence;
use rand::Rng;
use rand::prelude::SliceRandom;

#[inline]
fn marginal_price(x: U256, y: U256, a_in: U256, fee_ppm: U256) -> U256 {
    let r_num = U256::from(1_000_000u64) - fee_ppm;
    let r_den = U256::from(1_000_000u64);
    let ra = mul_div(a_in, r_num, r_den);
    let num = x + ra;
    mul_div(num, r_den, mul_div(y, r_num, U256::ONE))
}

fn assert_kkt(pools: &[(U256, U256)], split: &[U256], fee_ppm: u32) {
    let mut lambda = None::<U256>;
    for (&a, &(x, y)) in split.iter().zip(pools) {
        if a.is_zero() {
            continue;
        }
        let p = marginal_price(x, y, a, U256::from(fee_ppm));
        lambda = Some(lambda.map_or(p, |l| l.min(p)));
    }
    let lambda = lambda.expect("amount_in > 0");

    for (&a, &(x, y)) in split.iter().zip(pools) {
        let p = marginal_price(x, y, a, U256::from(fee_ppm));
        if a.is_zero() {
            let p0 = marginal_price(x, y, U256::ZERO, U256::from(fee_ppm));
            assert!(p0 >= lambda, "idle pool cheaper than λ");
        } else {
            let diff = if p > lambda { p - lambda } else { lambda - p };
            assert!(diff <= U256::ONE, "λ mismatch >1: {p} vs {lambda}");
        }
    }
}

proptest! {
    #![proptest_config(ProptestConfig {
        cases: 5_000,
        max_shrink_iters: 0,
        failure_persistence: Some(Box::new(
            FileFailurePersistence::WithSource("regressions1")
        )),
        ..ProptestConfig::default()
    })]
    #[test]
    fn kkt_holds_for_big_numbers(
        pools in prop::collection::vec(
            (1_000_000_000_000_000u128..=1e38 as u128,   // 1e15 … 1e38
             1_000_000_000_000_000u128..=1e38 as u128),
            2..=100
        ),
        fee_ppm in 0u32..=30_000,
        amount_in in 1_000_000_000u128..=1e35 as u128,   // 1e9 … 1e35
    ) {
        let pools_u256: Vec<(U256,U256)> = pools
            .iter().map(|&(x,y)| (U256::from(x),U256::from(y))).collect();
        let amt = U256::from(amount_in);

        let split = optimal_split(&pools_u256, amt);
        assert_eq!(split.iter().copied().sum::<U256>(), amt);

        assert_kkt(&pools_u256, &split, fee_ppm);
    }
}

proptest! {
    #![proptest_config(ProptestConfig {
        cases: 3_000,
        max_shrink_iters: 0,
        failure_persistence: Some(Box::new(
            FileFailurePersistence::WithSource("regressions")
        )),
        ..ProptestConfig::default()
    })]
    #[test]
    fn no_big_random_shift_improves_output(
        pools in prop::collection::vec(
            (1_000_000u128..=1e32 as u128,
             1_000_000u128..=1e32 as u128),
            2..=10
        ),
        fee_ppm in 0u32..=30_000,
        amount_in in 1_000_000u128..=1e30 as u128,
    ) {
        let pools_u256: Vec<(U256,U256)> = pools
            .iter().map(|&(x,y)| (U256::from(x),U256::from(y))).collect();
        let amt = U256::from(amount_in);

        let mut split = optimal_split(&pools_u256, amt);
        let best = total_output(&pools_u256, &split, fee_ppm);

        let mut rng = rand::rng();
        for _ in 0..100 {
            let n = split.len();
            let (i,j) = {
                let mut idx: Vec<_> = (0..n).collect();
                idx.shuffle(&mut rng); (idx[0], idx[1 % n])
            };
            if split[i].is_zero() { continue; }

            let max_delta = split[i].min(U256::from(1u64<<16))
                                    .min(amt / U256::from(1000u64)); // ≤0.1%
            if max_delta.is_zero() { continue; }
            let delta = U256::from(rng.random_range(1..=max_delta.to()));

            split[i] -= delta;
            split[j] += delta;
            let out = total_output(&pools_u256, &split, fee_ppm);
            assert!(out - (out / U256::from(1e24))  <= best, "shift improved output: old={best}, new={out}");
            split[i] += delta;
            split[j] -= delta;
        }
    }
}
