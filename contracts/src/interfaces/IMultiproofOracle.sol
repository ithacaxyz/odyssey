// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.13;

interface IMultiproofOracle {

    struct ImmutableArgs {
        uint88 proposalBond;
        uint40 challengeTime;
        uint88 proofReward;
        uint40 provingTime;
        uint64 treasuryFeePctWad;
        address treasury;
        uint16 emergencyPauseThreshold;
        uint40 emergencyPauseTime;
    }

    struct Challenge {
        uint256 blockNum;
        bytes32 outputRoot;
        uint256 index;
    }

    enum ProposalState {
        None,
        Unchallenged,
        Challenged,
        Rejected,
        Confirmed
    }

    struct ProposalData {
        address proposer;
        Challenge parent;
        uint40 deadline;
        uint8 version;
        ProposalState state;
        uint40 provenBitmap;
        address challenger;
    }

    struct ProofData {
        bytes publicValues;
        bytes proof;
    }

    struct PauseData {
        uint40 deadline;
        Challenge[] challenges;
    }
}
