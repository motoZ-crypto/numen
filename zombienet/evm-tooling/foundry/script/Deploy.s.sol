// SPDX-License-Identifier: MIT
pragma solidity ^0.8.24;

import "forge-std/Script.sol";
import {Token} from "../src/Token.sol";

/// Forge script:
///   forge script script/Deploy.s.sol \
///     --rpc-url crypto_node \
///     --private-key $ALITH_PRIVATE_KEY \
///     --broadcast
contract Deploy is Script {
    function run() external returns (Token token) {
        uint256 supply = 1_000_000 ether;
        vm.startBroadcast();
        token = new Token(supply);
        vm.stopBroadcast();
    }
}
