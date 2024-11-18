## Team Inputs

- `teamC`: amount the team can afford for challenges in an emergency
- `minReward`: minimum reward needed to incentivize provers
- `numP`: number of proof systems

## Parameters

There are a number of parameters that can be set by the team. These are the levers to pull in this system:
- `timeP`: time per proposal (ideal: short, for fast withdrawals)
- `timeC`: time to challenge proposal (ideal: long enough to be safe)
- `timeProve`: time to submit proof (ideal: long enough to give provers time)
- `proofTf`: treasury fee % on proven proposals (ideal: minimal as possible)
- `rejectTf`: treasury fee % on invalid proposals (ideal: minimal as possible)
- `timeEP`: time for emergency pause (ideal: short, so that emergency pause isn't too harmful if used)

These will end up outputting some objectively optimal values:
1) `bondC`: challengeBond (set to minimum that will incentivize provers)
2) `thresholdEP`: emergency pause threshold (set to maximum that the team can afford)
3) `bondP`: proposalBond (set to minimum that will provide DOS protection on par with challengeBond)

Specifically, the following constraints will set these optimal values:
```
bondC / numP * (1 - proofTf) >= minReward # 1
bondC * thresholdEP <= teamC # 2
bondP = bondC * proofTf / rejectTf # 3
```
We can simplify this optimization problem as follows...

Since we want to minimize `bondC`, we set it to the minimum value allowed by the first constraint above:
```
bondC = minReward * numP / (1 - proofTf)
```
Since `bondP` is defined relative to `bondC`, we can do the same:
```
bondP = bondC * proofTf / rejectTf = minReward * numP * proofTf / ((1 - proofTf) * rejectTf)
```
We can plug in `bondC` to get a formula for `thresholdEP`. Note that `teamC`, `minReward` and `numP` are all explicit constraints set by the team, so this gives us a defined ratio between `thresholdEP` and `proofTf`.
```
thresholdEP = teamC / bondC = teamC * (1 - proofTf) / (minReward * numP)
```

## Optimizations

Our calculations output three values that we want to optimize:
1) Minimize `ongoingETHLocked`.
2) Maximize `powProtection`.
3) Maximize `dosProtection`.

`ongoingETHLocked` is the size of the proposal bond times the number of outstanding proposal bonds on the happy path (challenge time divided by proposal time):
```
ongoingETHLocked = bondP * timeC / timeP
```
`powProtection` is provided by the fact that the honest party can perform the challenges while the contract is paused, and will increase their funds with each cycle. It can be calculated by taking the amount of team funds and multiplying it by the amount the funds grow each proving cycle to the power of the number of proving cycles per emergency pause time:
```
powProtection = teamC * ((1 + bondP / bondC) ** (timeEP / timeProve))
```
`dosProtection` is the amount of ETH is costs to artificially trigger an emergency pause. This is enforced by the treasury fees. Since we calculated `bondP` above in order to keep the treasury fees for both types equal, we can just use the successful proof case to calculate how much fees would be charged for `thresholdEP` challenges:
```
dosProtection = (minRewards * numP * proofTf * thresholdEP) / (1 - proofTf)
```

## Process

1) Copy the [Multiproof Parameter Calculator](https://docs.google.com/spreadsheets/d/1csqvtQxZNtQxwJ72du3oy5BVA54gGalmNDK0lA6h2Gc/edit?gid=0#gid=0) spreadsheet.

2) Plug in the `minReward`, `numP`, and `teamC` values.

3) Try a set of variables, which will output optimal values for `bondP`, `bondC`, and `thresholdEP`, as well as the three outcomes.

4) If the outcomes are not satisfactory, use the suggestions in Column E to adjust the variables. Our recommended results are:
- `ongoingETHLocked`: whatever you're comfortable with, shouldn't need to be higher than 50 ETH
- `powProtection`: at least 10x your TVL AND more than 400,000 ETH
- `dosProtection`: at least 250 ETH
