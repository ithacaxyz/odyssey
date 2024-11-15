// SPDX-License-Identifier: UNLICENSED
pragma solidity ^0.8.13;

import { IProver } from "./interfaces/IProver.sol";
import { IMultiproofOracle } from "./interfaces/IMultiproofOracle.sol";

contract MultiproofOracle is IMultiproofOracle {

    ///////////////////////////////
    //////// STATE VARIABLES /////
    ///////////////////////////////

    uint8 public constant VERSION = 1;

    uint256 immutable proposalBond;
    uint256 immutable challengeTime;

    uint256 immutable proofReward; // to challenge, we must bond `proofReward * provers.length`
    uint256 immutable provingTime;

    mapping(uint256 blockNum => mapping(bytes32 outputRoot => ProposalData[])) public proposals;
    IProver[] public provers;

    uint256 treasuryFeePctWad;
    address treasury;

    uint256 immutable emergencyPauseThreshold;
    uint256 immutable emergencyBond;
    uint256 immutable emergencyPauseTime;
    address[] emergencyPauses;
    mapping(address pauser => PauseData) public emergencyPauseData;

    bool emergencyShutdown;

    ///////////////////////////////
    ///////// CONSTRUCTOR /////////
    ///////////////////////////////

    constructor(IProver[] memory _provers, uint256 _initialBlockNum, bytes32 _initialOutputRoot, ImmutableArgs memory _args) {
        require(_provers.length < 40); // proven bitmap has to fit in uint40
        provers = _provers;

        // set params
        require(_args.treasuryFeePctWad < 1e18, "treasury fee must be less than 100%");
        proposalBond = _args.proposalBond;                       // sane default: 1 ETH
        challengeTime = _args.challengeTime;                     // sane default: 1 day
        proofReward = _args.proofReward;                         // sane default: 1 ETH
        provingTime = _args.provingTime;                         // sane default: 1 day
        treasuryFeePctWad = _args.treasuryFeePctWad;             // sane default: 20%
        treasury = _args.treasury;
        emergencyPauseThreshold = _args.emergencyPauseThreshold; // sane default: 100
        emergencyBond = _args.emergencyBond;                     // sane default: 100 ETH
        emergencyPauseTime = _args.emergencyPauseTime;           // sane default: 10 days

        // initialize anchor state with Confirmed status
        proposals[_initialBlockNum][_initialOutputRoot].push(ProposalData({
            proposer: address(0),
            parent: Challenge({
                blockNum: 0,
                outputRoot: bytes32(0),
                index: 0
            }),
            deadline: 0,
            version: VERSION,
            state: ProposalState.Confirmed,
            provenBitmap: 0,
            challenger: address(0)
        }));
    }

    ///////////////////////////////
    ////////// LIFECYCLE //////////
    ///////////////////////////////

    function propose(Challenge memory parent, uint256 blockNum, bytes32 outputRoot) public payable {
        require(msg.value == proposalBond, "incorrect bond amount");

        proposals[blockNum][outputRoot].push(ProposalData({
            proposer: msg.sender,
            parent: parent,
            deadline: uint40(block.timestamp + challengeTime),
            version: VERSION,
            state: ProposalState.Unchallenged,
            provenBitmap: 0,
            challenger: address(0)
        }));
    }

    function challenge(uint256 blockNum, bytes32 outputRoot, uint256 index) public payable {
        require(msg.value == proofReward * provers.length, "incorrect bond amount");

        ProposalData storage proposal = proposals[blockNum][outputRoot][index];
        require(proposal.state == ProposalState.Unchallenged, "can only challenge unchallenged proposals");
        require(proposal.deadline > block.timestamp, "deadline passed");

        proposals[blockNum][outputRoot][index].deadline = uint40(block.timestamp + provingTime);
        proposals[blockNum][outputRoot][index].state = ProposalState.Challenged;
        proposals[blockNum][outputRoot][index].challenger = msg.sender;
    }

    function prove(uint256 blockNum, bytes32 outputRoot, uint256 index, ProofData[] memory proofs) public {
        ProposalData storage proposal = proposals[blockNum][outputRoot][index];
        require(proposal.state == ProposalState.Challenged, "can only prove challenged proposals");
        require(proposal.deadline > block.timestamp, "deadline passed");

        // verify ZK proofs
        require(proofs.length == provers.length, "incorrect number of proofs");
        uint successfulProofCount;
        for (uint256 i = 0; i < proofs.length; i++) {
            if (proposal.provenBitmap & (1 << i) != 0) {
                continue;
            }

            if (provers[i].verify(proofs[i].publicValues, proofs[i].proof)) {
                proposal.provenBitmap |= uint40(1 << i);
                successfulProofCount++;
            }
        }

        if (successfulProofCount > 0) {
            uint rewards = proofReward * successfulProofCount;
            uint treasuryFee = rewards * treasuryFeePctWad / 1e18;
            payable(treasury).transfer(treasuryFee);
            payable(msg.sender).transfer(rewards - treasuryFee);
        }
    }

    function finalize(uint256 blockNum, bytes32 outputRoot, uint256 index) public {
        require(emergencyPauses.length == 0, "no finalizing when emergency paused");

        ProposalData storage proposal = proposals[blockNum][outputRoot][index];
        require(!isFinalized(proposal.state), "proposal already finalized");

        Challenge memory parent = proposal.parent;
        ProposalData storage parentProposal = proposals[parent.blockNum][parent.outputRoot][parent.index];
        if (!isFinalized(parentProposal.state)) {
            finalize(parent.blockNum, parent.outputRoot, parent.index);

            // extra safety check
            require(isFinalized(parentProposal.state), "parent not finalized");
        }

        uint successfulProofCount;
        for (uint i = 0; i < provers.length; i++) {
            if (proposal.provenBitmap & (1 << i) == 1) {
                successfulProofCount++;
            }
        }

        // If the parent was Rejected, the child should be Rejected.
        if (parentProposal.state == ProposalState.Rejected) {
            proposal.state = ProposalState.Rejected;

            if (proposal.challenger != address(0)) {
                payable(proposal.challenger).transfer(proposalBond + proofReward * (provers.length - successfulProofCount));
            } else {
                payable(msg.sender).transfer(proposalBond);
            }

            return;
        }

        require(proposal.deadline < block.timestamp, "deadline not passed");

        // If we make it here, we know:
        // - the parent was Confirmed
        // - the proposal is either Unchallenged or Challenged
        // - the proposal deadline has passed, so it can be processed

        // If the proposal was unchallenged, or challenged and fully proven, it is Confirmed.
        // In both cases, there are no additional rewards (since if successfulProofCount == provers.length,
        // provers have taken all add'l rewards directly), so the proposer gets their bond back.
        if (proposal.state == ProposalState.Unchallenged || successfulProofCount == provers.length) {
            proposal.state = ProposalState.Confirmed;
            payable(proposal.proposer).transfer(proposalBond);
            return;
        }

        // Otherwise, it means the proposal was challenged and not fully proven.
        proposal.state = ProposalState.Rejected;

        // The treasury fee here MUST be substantial enough to deter the attack where an attacker:
        // (a) proposes false root, (b) challenges self, (c) emergency pause to DOS the system.
        // We can calculate this cost as `proposalBond * treasuryFeePctWad / 1e18 * emergencyPauseThreshold`.
        uint treasuryFee = proposalBond * treasuryFeePctWad / 1e18;
        uint proposalBondRewards = proposalBond - treasuryFee;
        uint proofRewards = proofReward * (provers.length - successfulProofCount);

        payable(treasury).transfer(treasuryFee);
        payable(proposal.challenger).transfer(proposalBondRewards + proofRewards);

        // If any proofs were proven, we have an on chain bug and need to shut down to address it.
        if (proposal.provenBitmap != 0) {
            emergencyShutdown = true;
        }
    }

    ///////////////////////////////
    /////////// EMERGENCY //////////
    ///////////////////////////////

    // This function exists to block a Proof of Whale attack.
    // If an attacker is proposing too many false roots, this allows us to emergency pause the contract
    // by attesting to that at least 100 false roots have already been challenged.
    // If any of the challenges are proven, the pauser loses their full bond.
    function emergencyPause(Challenge[] memory challenges) public payable {
        require(msg.value == emergencyBond, "incorrect bond amount");
        require(emergencyPauseData[msg.sender].deadline == 0, "already in emergency pause");
        require(challenges.length >= emergencyPauseThreshold, "not enough challenges");

        for (uint i = 0; i < challenges.length; i++) {
            Challenge memory c = challenges[i];
            require(proposals[c.blockNum][c.outputRoot][c.index].state == ProposalState.Challenged, "proposal not confirmed");
        }

        emergencyPauses.push(msg.sender);
        emergencyPauseData[msg.sender] = PauseData({
            deadline: uint40(block.timestamp + emergencyPauseTime),
            challenges: challenges
        });
    }

    function endPause(address pauser) public {
        require(emergencyPauseData[pauser].deadline > 0, "not in emergency pause");

        if (block.timestamp < emergencyPauseData[pauser].deadline) {
            require(msg.sender == pauser, "deadline not expired, only pauser can end pause");
        }

        if (allEmergencyPauseChallengesAreUnproven(msg.sender)) {
            payable(pauser).transfer(emergencyBond);
        }

        delete emergencyPauseData[msg.sender];
        for (uint i = 0; i < emergencyPauses.length; i++) {
            if (emergencyPauses[i] == msg.sender) {
                emergencyPauses[i] = emergencyPauses[emergencyPauses.length - 1];
                emergencyPauses.pop();
                break;
            }
        }
    }

    ///////////////////////////////
    //////////// VIEWS ////////////
    ///////////////////////////////

    function isFinalized(ProposalState state) public pure returns (bool) {
        require(state != ProposalState.None, "proposal doesn't exist");
        return state == ProposalState.Confirmed || state == ProposalState.Rejected;
    }

    // This can be called on chain to check if a block number and output root have been confirmed.
    // TODO: Do some gas testing to see the max length of proposals that can be checked
    // without running out of gas. We probably wouldn't want to use this anywhere mission critical.
    function isValidProposal(uint256 blockNum, bytes32 outputRoot) public view returns (bool) {
        uint proposalsLength = proposals[blockNum][outputRoot].length;
        for (uint i = 0; i < proposalsLength; i++) {
            if (isValidProposal(blockNum, outputRoot, i)) return true;
        }

        return false;
    }

    function isValidProposal(uint256 blockNum, bytes32 outputRoot, uint256 index) public view returns (bool) {
        return proposals[blockNum][outputRoot][index].state == ProposalState.Confirmed;
    }

    function allEmergencyPauseChallengesAreUnproven(address pauser) public view returns (bool) {
        uint length = emergencyPauseData[pauser].challenges.length;
        for (uint i = 0; i < length; i++) {
            Challenge memory c = emergencyPauseData[pauser].challenges[i];
            if (proposals[c.blockNum][c.outputRoot][c.index].provenBitmap != 0) {
                return false;
            }
        }
        return true;
    }
}
