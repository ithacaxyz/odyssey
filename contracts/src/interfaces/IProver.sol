// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.13;

interface IProver {
    function verify(bytes memory publicValues, bytes memory proof) external view returns (bool);
}
