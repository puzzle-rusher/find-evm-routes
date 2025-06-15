use alloy::sol;

sol! {
    #[sol(rpc)]
    interface IUniswapV2Factory {
        function getPair(address tokenA, address tokenB) external view returns (address pair);
        function allPairs(uint256) external view returns (address);
        function allPairsLength() external view returns (uint256);
    }

    #[sol(rpc)]
    interface IUniswapV2Pair {
        function token0() external view returns (address);
        function token1() external view returns (address);
        function getReserves()
            external view
            returns (uint112 reserve0, uint112 reserve1, uint32 blockTimestampLast);
    }
}
