// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.13;

import { MultiproofOracle } from "src/MultiproofOracle.sol";
import { MockProver } from "src/mocks/MockProver.sol";
import { IProver } from "src/interfaces/IProver.sol";
import { IMultiproofOracle } from "src/interfaces/IMultiproofOracle.sol";
import { Test, console } from "forge-std/Test.sol";

contract MultiproofOracleTest is Test {
    MultiproofOracle public oracle;
    IMultiproofOracle.Challenge public anchor;
    uint96 proposedBlockNum = 1;
    bytes32 proposedOutputRoot = bytes32(uint256(1));

    function setUp() public {
        IProver[] memory provers = new IProver[](3);
        for (uint i = 0; i < provers.length; i++) {
            provers[i] = IProver(address(new MockProver()));
        }

        // set based on defaults here:
        // https://docs.google.com/spreadsheets/d/1csqvtQxZNtQxwJ72du3oy5BVA54gGalmNDK0lA6h2Gc/edit?gid=0#gid=0
        IMultiproofOracle.ImmutableArgs memory args = IMultiproofOracle.ImmutableArgs({
            proposalBond: uint88(3 ether),
            challengeTime: uint40(12 hours),
            proofReward: uint88(1 ether),
            provingTime: uint40(1 days),
            treasuryFeePctWad: uint64(0.5e18),
            treasury: address(makeAddr("treasury")),
            emergencyPauseThreshold: uint16(200),
            emergencyPauseTime: uint40(10 days)
        });
        oracle = new MultiproofOracle(provers, 0, bytes32(0), args);
        vm.deal(address(oracle), 100 ether);

        anchor = IMultiproofOracle.Challenge({
            outputRoot: bytes32(0),
            index: 0
        });
    }

    // propose wait finalize, confirmed
    function testMultiproof_unchallengedCanFinalize() public {
        oracle.propose{value: oracle.proposalBond()}(anchor, proposedBlockNum, proposedOutputRoot);
        vm.warp(block.timestamp + oracle.challengeTime() + 1);
        oracle.finalize(proposedOutputRoot, 0);
        assert(oracle.isValidProposal(proposedOutputRoot));
    }

    // propose, try finalize: deadline not passed
    function testMultiproof_cannotFinalizeBeforeChallengeTime() public {
        oracle.propose{value: oracle.proposalBond()}(anchor, proposedBlockNum, proposedOutputRoot);

        vm.expectRevert("deadline not passed");
        oracle.finalize(proposedOutputRoot, 0);
        assert(!oracle.isValidProposal(proposedOutputRoot));
    }

    // propose, challenge, wait, try finalize: deadline not passed
    function testMultiproof_cannotFinalizeBeforeProvingDeadline() public {
        oracle.propose{value: oracle.proposalBond()}(anchor, proposedBlockNum, proposedOutputRoot);
        oracle.challenge{value: oracle.proofReward() * 3}(proposedOutputRoot, 0);

        vm.expectRevert("deadline not passed");
        oracle.finalize(proposedOutputRoot, 0);
        assert(!oracle.isValidProposal(proposedOutputRoot));
    }

    // propose, challenge, no proof, finalize: rejected
    function testMultiproof_rejectedAfterUnprovenChallenge() public {
        oracle.propose{value: oracle.proposalBond()}(anchor, proposedBlockNum, proposedOutputRoot);
        oracle.challenge{value: oracle.proofReward() * 3}(proposedOutputRoot, 0);

        vm.warp(block.timestamp + oracle.provingTime() + 1);
        oracle.finalize(proposedOutputRoot, 0);

        (,,,, IMultiproofOracle.ProposalState proposalState,,,) = oracle.proposals(proposedOutputRoot, 0);
        assertEq(uint8(proposalState), uint8(IMultiproofOracle.ProposalState.Rejected));
        assert(!oracle.emergencyShutdown());
    }

    // failed proof rejected but doesn't shutdown
    function testMultiproof_rejectedAndActiveAfterFailedProof() public {
        oracle.propose{value: oracle.proposalBond()}(anchor, proposedBlockNum, proposedOutputRoot);
        oracle.challenge{value: oracle.proofReward() * 3}(proposedOutputRoot, 0);

        // all failed proofs
        IMultiproofOracle.ProofData[] memory proofs = _generateProofs(false, true);
        oracle.prove(proposedOutputRoot, 0, proofs);

        vm.warp(block.timestamp + oracle.provingTime() + 1);
        oracle.finalize(proposedOutputRoot, 0);

        (,,,, IMultiproofOracle.ProposalState proposalState,,,) = oracle.proposals(proposedOutputRoot, 0);
        assertEq(uint8(proposalState), uint8(IMultiproofOracle.ProposalState.Rejected));
        assert(!oracle.emergencyShutdown());
    }

    // semi proven proof rejected and shutdown
    function testMultiproof_rejectedAndShutdownAfterSemiprovenProof() public {
        oracle.propose{value: oracle.proposalBond()}(anchor, proposedBlockNum, proposedOutputRoot);
        oracle.challenge{value: oracle.proofReward() * 3}(proposedOutputRoot, 0);

        // all failed proofs
        IMultiproofOracle.ProofData[] memory proofs = _generateProofs(false, false);
        oracle.prove(proposedOutputRoot, 0, proofs);

        vm.warp(block.timestamp + oracle.provingTime() + 1);
        oracle.finalize(proposedOutputRoot, 0);

        (,,,, IMultiproofOracle.ProposalState proposalState,,,) = oracle.proposals(proposedOutputRoot, 0);
        assertEq(uint8(proposalState), uint8(IMultiproofOracle.ProposalState.Rejected));
        assert(oracle.emergencyShutdown());
    }

    // fully proven proof confirmed
    function testMultiproof_confirmedAndActiveAfterSuccessfulProof() public {
        oracle.propose{value: oracle.proposalBond()}(anchor, proposedBlockNum, proposedOutputRoot);
        oracle.challenge{value: oracle.proofReward() * 3}(proposedOutputRoot, 0);

        // all failed proofs
        IMultiproofOracle.ProofData[] memory proofs = _generateProofs(true, false);
        oracle.prove(proposedOutputRoot, 0, proofs);

        vm.warp(block.timestamp + oracle.provingTime() + 1);
        oracle.finalize(proposedOutputRoot, 0);

        (,,,, IMultiproofOracle.ProposalState proposalState,,,) = oracle.proposals(proposedOutputRoot, 0);
        assertEq(uint8(proposalState), uint8(IMultiproofOracle.ProposalState.Confirmed));
        assert(!oracle.emergencyShutdown());
    }

    // propose, propose child, challegen parent, wait: both rejected
    function testMultiproof_rejectedParentRejectsChild() public {
        oracle.propose{value: oracle.proposalBond()}(anchor, proposedBlockNum, proposedOutputRoot);
        bytes32 newOutputRoot = keccak256(abi.encode(proposedOutputRoot));

        IMultiproofOracle.Challenge memory parent = IMultiproofOracle.Challenge({
            outputRoot: proposedOutputRoot,
            index: 0
        });
        oracle.propose{value: oracle.proposalBond()}(parent, proposedBlockNum + 1, newOutputRoot);

        oracle.challenge{value: oracle.proofReward() * 3}(proposedOutputRoot, 0);

        vm.warp(block.timestamp + oracle.provingTime() + 1);
        oracle.finalize(newOutputRoot, 0);

        (,,,, IMultiproofOracle.ProposalState proposalState,,,) = oracle.proposals(proposedOutputRoot, 0);
        assertEq(uint8(proposalState), uint8(IMultiproofOracle.ProposalState.Rejected));

        (,,,, proposalState,,,) = oracle.proposals(newOutputRoot, 0);
        assertEq(uint8(proposalState), uint8(IMultiproofOracle.ProposalState.Rejected));
    }

    // propose 200+, challenge 200+, emergency pause
    function testMultiproof_emergencyPause() public {
        uint emergencyThreshold = oracle.emergencyPauseThreshold();
        IMultiproofOracle.Challenge[] memory challenges = new IMultiproofOracle.Challenge[](emergencyThreshold);
        for (uint i; i < emergencyThreshold; i++) {
            challenges[i] = IMultiproofOracle.Challenge({
                outputRoot: proposedOutputRoot,
                index: i
            });
            oracle.propose{value: oracle.proposalBond()}(anchor, proposedBlockNum, proposedOutputRoot);
            oracle.challenge{value: oracle.proofReward() * 3}(proposedOutputRoot, i);
        }
        oracle.emergencyPause(challenges);

        assert(oracle.emergencyPaused());
        assertEq(oracle.emergencyPauseDeadline(), block.timestamp + oracle.emergencyPauseTime());
    }

    function testMultiproof_emergencyShutdownInContradiction() public {
        bytes32 outputRoot2 = keccak256(abi.encode(proposedOutputRoot));
        oracle.propose{value: oracle.proposalBond()}(anchor, proposedBlockNum, proposedOutputRoot);
        oracle.propose{value: oracle.proposalBond()}(anchor, proposedBlockNum, outputRoot2);

        vm.warp(block.timestamp + oracle.challengeTime() + 1);

        oracle.finalize(proposedOutputRoot, 0);
        oracle.finalize(outputRoot2, 0);

        oracle.triggerEmergencyShutdown(proposedOutputRoot, 0, outputRoot2, 0);
        assert(oracle.emergencyShutdown());
    }

    // // TODO: Add tests to ensure all the bonds flow to the right people

    receive() external payable {}

    function _generateProofs(bool allTrue, bool allFalse) internal pure returns (IMultiproofOracle.ProofData[] memory) {
        IMultiproofOracle.ProofData[] memory proofs = new IMultiproofOracle.ProofData[](3);
        for (uint i = 0; i < proofs.length; i++) {
            bytes memory proof = allTrue || (!allFalse && i > 0) ? bytes("proof") : bytes("");
            proofs[i] = IMultiproofOracle.ProofData({
                publicValues: bytes(""),
                proof: proof
            });
        }
        return proofs;
    }
}
