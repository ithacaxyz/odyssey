// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.13;

import { IProver } from "src/interfaces/IProver.sol";

contract MockProver is IProver {
    function verify(bytes memory publicValues, bytes memory proof) external view returns (bool) {
        if (proof.length == 0) {
            return false;
        }
        return true;
    }
}
