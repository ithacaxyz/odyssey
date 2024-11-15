# Proof of Whale

One of the constraints for the system is:

> Proof of Whale: An attacker should not be able to force through an invalid proposal by using a large amount of funds.

There are two possible directions to protect against this:

1) Create a mechanism that, under attack situations, the contract can be put in an emergency state to allow the excess proposals to be dealt with before they are confirmed. This has the negative of causing a disturbance in normal operations if it occurs.

2) Ensure the ratio of `proposalBond` to `challengeBond` is high enough that the risk is diminished. While in a naive implementation, an expensive `proposalBond` makes the ongoing happy path of proposing for the chain quite expensive, an EIP1559 style mechanism can be added to ensure that the `proposalBond` is only high when an attack is taking place. This has the negative of not completely protecting against Proof of Whale, and just shifting the ratio in the honest party's favor.

I would recommend the first option, but will lay out both options below.

### Option 1: Emergency State

Because challenges are easy to perform (no proof generation is required), the only bottleneck on challenging invalid state transitions is the cost of the `challengeBond`.

Option 1 uses two additional immutable parameters called `EMERGENCY_THRESHOLD` and `emergencyBond`. The threshold determines a minimum number of ongoing Challenged proposals that must coexist for the state to be triggered. The bond is the amount that must be bonded in order to trigger the emergency state (and is only refunded if ALL of the included proposals are in fact invalid).

In this case, a function could be implemented that takes in an array of `EMERGENCY_THRESHOLD` or more proposals. Each of these proposals should already be in the `Challenged` state. Payment of `emergencyBond` is held, and the contract is put into emergency state.

Putting the contract in emergency state will allow the caller to mark other proposals as `toChallenge`. These are proposals they believe are invalid but do not yet have the funds to challenge. Only when this `toChallenge` list is back to 0 (or a specific amount of time has passed, for example 10 days) will the contract leave the emergency state and will proposals be allowed to be finalized again.

While this does create the risk of pausing new state roots from being posted for an extended time, it is the preferred strategy because:
- There is no possible attack that doesn't cost the attacker significant funds. If they don't challenge their own proposals, they will lose at least `EMERGENCY_THRESHOLD * challengeBond`. If they do challenge their own proposals and then set the emergency state themselves, they will lose `emergencyBond`.
- All the defender's funds will be returned each `provingTime` (approx 1 day). This means the defender will be able to double their total funds every day. It will become incredible expensive for an attacker to maintain this.
- Because the defender doubles their funds every 24 hours, a maximum emergency time of 10 days would allow for them to double 10 times, meaning that the attacker would need `2 ** 10 = 1024` times the funds to perform an attacker, which is infeasible.
- The sequencing of the chain is not interrupted, as that is performed in an unrelated process by op-batcher. The only thing that is paused is the ability to perform withdrawals.

### Option 2: EIP1559 Proposal Bonds

The constraint requires that, in the situation where such an attack is taking place, `proposalBond` is sufficiently greater than the `challengeBond` such that a malicious actor can't simply outspend the proposer to have a state root accepted.

The naive solution is to set `proposalBond = challengeBond * HIGH_MULTIPLIER`. But this would be very inconvenient because on the happy path, the proposer will need to deposit the `proposalBond` along with each state root. If we set a new state root each hour, with a `challengeTime` of 1 day, this would require `24 * proposalBond` to be locked at all times. This means that a high value for `proposalBond` requires a high value of continuously locked ETH.

Fortunately, we can vary the `proposalBond` based on whether an attack like this is occurring. A simple EIP1559 style mechanism with the right parameters can be very helpful.

As an example, let's say the formula for `proposalBond` was the following, which sets `proposalBond = challengeBond` for the first 100 proposals, and then slowly scales up to a maximum of `proposalBond = challengeBond * 10` at 1000 proposals.
```
proposalBondMultiplier = min(max(1, proposalsInPast24Hours * 0.01), 10)
proposalBond = challengeBond * proposalBondMultiplier
```
The result is that, to create 1000 proposals, the cost will be `5050 * challengeBond`, which is > 5X Proof of Whale protection. This may be considered sufficient for some chains, based on the ratio of their TVL to the assets available to perform challenges.

However, to maintain the chain in the happy state, there will only be 24 proposals per day, so the cost will just be `24 * challengeBond`, which is a small fraction of the available assets.

Finally, there is one other attack to consider. An attacker could stuff the chain with honest proposals to increase the `proposalBond`, without risking any funds (because the proposals are honest). This would make it more expensive to maintain the chain.

However, because of the graduating cost of proposing, the cost to perform such an attack would require keeping assets locked in the contract to increase the price each day. This is substantially higher than the cost incurred by the proposer. For example, if the chain is proposing 24 proposals per day, we could increase the price to a maximum of `240 * challengeBond` per day by inserting 1000 honest proposals first. But these honest proposals would cost `5050 * challengeBond` to create, which is > 20x more, and would provide sufficient deterrence.
