# Multiproof Background

`OptimismPortal2.sol` is responsible for managing all withdrawals from L2 to L1. It validates these withdrawals by confirming that a hash of the withdrawal exists in the storage of the `L2ToL1MessagePasser.sol` contract on L2. In order to be confident in this proof, we need a trusted L2 output root (the state root with some additional data).

There have been two historical implementations of the OP Stack:
- Stage 0: `L2OutputOracle.sol` is queried for these output roots, which are simply added by a trusted role.
- Stage 1: `DisputeGameFactory.sol` is queried for these output roots, which returns the address of a contract that is holding the dispute game for that root. This contract is queried to ensure that the game has passed (and sufficient time has passed) before the withdrawal is allowed.

However, the Stage 1 implementation above has a number of problems:

**1) Fraud proofs are hard.** Look at `FaultDisputeGame.sol` or `MIPS.sol` and you'll see that there is a high risk of bugs. Since such a bug would be catastrophic without safety wheels, this is a serious risk when pushing for Stage 2.

**2) Not many provable bugs.** Stage 2 requires that the Security Council can only force upgrades from on chain provable bugs. While these are possible in a single proof system, they are much more likely in a multiproof system.

**3) Proof of Whale.** Because the existing solution requires a bisection game to play the dispute game, the economic incentives can be complex. For example, if there were not safeguards, a sufficiently funded adversary could win the dispute game by outspending the honest party.

## Why Multiproof?

By creating an implementation that looks at multiple proofs, we solve:

1) If an individual proof game has a bug, the other proofs can ensure that it doesn't lead to loss of funds.

2) If we have multiple proof games, the world of on chain provable bugs grows dramatically, as any bug in one game can be shown to not match the results of another game.

Additionally, since multiple proofs are being used, we can more safely rely on zkVMs and TEEs, which solves:

3) We don't need to bisect across blocks to generate proofs, so we can avoid any Proof of Whale risk.
