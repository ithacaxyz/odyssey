# MultiproofOracle.sol Contract Spec

`MultiproofOracle.sol` is responsible for managing the aggregate proof logic.

### Constraints & Solutions

For any multiproof system to be secure...

**1) No state roots should be ever accepted unless all proofs agree they are valid.**

Solution: If a state transition passes all proofs, it is accepted. If it fails all proofs, it is rejected. In any other situation, it is rejected and creates an emergency pause for further investigation.

**2) Challengers and defenders must always have sufficient time to perform their role.**

Solution: The time permitted for challengers and defenders should be set as parameters `challengeTime` and `provingTime`, based on the upper bounds of how long these two acts should take. (~1 day seems like a reasonable default value for both.)

**3) Invalid proposals cannot be used to block valid proposals.**

Solution: There must be no limits on proposals. Each proposal can be evaluated independently. This requires that each proposal is evaluated relative to its parent, and that its parent must be resolved as true for it to be accepted.

**4) Proposing state transitions that cannot be ZK proven should not cause any harm to the integrity of the chain.**

Solution: Challengers should not be required to generate ZK proofs to challenge a claim, or else this class of attacks opens up. Instead, challenging should be as simple as possible, and proofs should be used to "defend" a proposal.

**5) Invalid challenges should not impair the ability to perform a valid challenge.**

Solution: Challenges will be completely generic, simply depositing funds that are available to win if someone proves the state transition. In this way, there is no such thing as a "valid" or "invalid" challenge, as all challenges of a given proposal are fungible.

**6) Proof of Whale: An attacker should not be able to force through an invalid proposal by using a large amount of funds.**

Solution: There are a few possible options to accomplish this goal. See [proofofwhale.md](./proofofwhale.md) for details.

**7) Contradictory proofs should shut down the system.**

Solution: If we are able to prove two different state roots for the same block, the system can be emergency paused.

## Architecture

All proposals are tracked in one mapping. As we can see, this follows the solution of allowing unlimited proposals to be created, even for a given block number & output root pair.

```solidity
mapping(uint256 blockNum => mapping(bytes32 outputRoot => ProposalData[])) public proposals;

struct ProposalData {
    address proposer;
    uint96 proposalBond;
    Parent parent; // block number, output root, and index
    uint40 deadline;
    uint8 version;
    ProposalState state; // None, Unchallenged, Challenged, Rejected, or Confirmed
    uint40 provenBitmap;
    address challenger;
}
```

The contract is initialized with some immutable values representing the `provers` (list of subproof contracts), `challengeTime` (seconds for which a proposal can be challenged), `provingTime` (time to generate a proof to defend a proposal), `proofReward` (reward per proof, which is multiplied by the number of provers to get the `challengeBond`), and the `proposalBond`.

We also initialize the `proposals` mapping with a Confirmed anchor state to act as the parent for the first proposal.

The lifecycle for each new proposal follows four steps:
1) Propose
2) Challenge (if someone disagrees with the proposal)
3) Prove (if someone defends against a challenge)
4) Finalize

```solidity
function propose(Parent memory parent, uint256 blockNum, bytes32 outputRoot) external payable;
```
- requires that `msg.value == current proposalBond`
- pushes a new proposal into the mapping, currently in an Unchallenged state
- sets the deadline for the proposal to `block.timestamp + challengeTime`

```solidity
function challenge(uint256 blockNum, bytes32 outputRoot, uint256 index) external payable;
```
- requires that `msg.value == proofReward * provers.length`
- sets the proposal state to Challenged and the challenger to msg.sender
- updates the proposal deadline to `block.timestamp + provingTime`

```solidity
function prove(uint256 blockNum, bytes32 outputRoot, uint256 index, ProofData[] memory proofs) external;
```
- requires that the proposal is Challenged and deadline hasn't passed
- for any subproof that hasn't already been proven on this challenge, call prover.verify(). if it passes, set it as proven and pay msg.sender `proofReward`.

```solidity
function finalize(uint256 blockNum, bytes32 outputRoot, uint256 index) external;
```
- require the proposal is either Unchallenged or Challenged, and deadline has passed
- if the parent is not Confirmed or Rejected, recursively call finalize() on it
- if the parent is Rejected:
    - set the proposal to Rejected & pay out bonds to the challenger (or msg.sender if Unchallenged)
- if the parent is Confirmed:
    - if the proposal is Unchallenged, set it to Confirmed & refund proposer
    - if the proposal is Challenged and all proved, set it to Confirmed & refund proposer (all individual `proofReward`s have already been paid in `prove()`)
    - if the proposal is Challenged and none proved, set it to Rejected & send all funds to challenger
    - if the proposal is Challenged are partially proved, set it to Rejected, send challenger the remaining bonds, and set `emergencyPause = true`.

Additionally, a public function exists to put the contract into emergency pause state if we have contradictory proofs.

```solidity
function emergencyPause(uint blockNum, bytes32 outputRoot1, uint256 index1, bytes32 outputRoot2, uint256 index2) external;
```
- require that both proposals are Confirmed
- require that outputRoot1 != outputRoot2
- set `emergencyPause = true`
