# Odyssey

<!-- [![Crates.io][crates-badge]][crates-io] -->
<!-- [![Downloads][downloads-badge]][crates-io] -->
[![MIT License][mit-badge]][mit-url]
[![Apache-2.0 License][apache-badge]][apache-url]
[![CI Status][actions-badge]][actions-url]

## What is Odyssey?

Odyssey is a testnet OP Stack rollup aimed at enabling experimentation of bleeding edge Ethereum Research.
Odyssey is __not__ a fork of reth.
Odyssey implements traits provided by the [reth node builder API](https://paradigmxyz.github.io/reth/docs/reth_node_builder/index.html), allowing implementation of precompiles and instructions of experimental EIPs without forking the node.

Specifically, Odyssey currently implements the following EIPs:
 - [EIP-7702](https://eips.ethereum.org/EIPS/eip-7702): Set EOA account code.
 - [RIP-7212](https://ethereum-magicians.org/t/eip-7212-precompiled-for-secp256r1-curve-support/14789): Precompile for secp256r1 curve support.
 - [EIP-2537](https://eips.ethereum.org/EIPS/eip-2537): Precompiles for BLS12-381 curve operations.

Odyssey also implements the EIPs for EOF, or [The EVM Object Format](https://evmobjectformat.org/).

### Why Odyssey?

Odyssey has 2 goals:
1. Showcase Reth's performance at the extremes. We intend to launch a hosted version of Odyssey on [Conduit](https://conduit.xyz/), targeting 50mgas/s, and eventually ramping up to 1ggas/s and beyond. In the process we hope to hit the state growth performance bottleneck, and discover ways to solve it. If our hosted chains end up getting too big, we may possibly restart the experiment from zero, and try again.
2. Showcase how Reth's modular architecture can serve as a distribution channel for research ideas. Specifically,
Odyssey's node extensions were chosen for their ability to enable applications that enhance the onchain user experience, and
drastically reduce cost for existing applications that improve UX.

### Odyssey Testnet

> [!TIP]
> [The Odyssey Testnet](https://www.ithaca.xyz/updates/odyssey#odyssey-chapter-1-is-live-on-testnet) is now live on Sepolia and is built with Reth, the OP Stack, and [deployed on Conduit](https://app.conduit.xyz/published/view/odyssey).

### Odyssey Local Development

Odyssey can be run locally for development and testing purposes. To do this, the binary can be run with the `--dev` flag, which will start the node with a development configuration.

First, odyssey should be built locally:
```bash
git clone https://github.com/ithacaxyz/odyssey
cd odyssey
cargo install --path bin/odyssey
```

```bash
odyssey node --chain etc/odyssey-genesis.json --dev --http --http.api all
```

This will start the node with a development configuration, and expose the HTTP API on `http://localhost:8545`.

To use EOF-enabled foundry, use [forge-eof](https://github.com/paradigmxyz/forge-eof) and follow installation instructions.

### Running Odyssey

Running Odyssey will require running additional infrastructure for the archival L1 node. These instructions are a guide for
running the Odyssey OP-stack node only.

For instructions on running the full Odyssey OP stack, including the L1 node, see the [Reth book section on running the OP stack](https://paradigmxyz.github.io/reth/run/optimism.html), using the `odyssey` binary instead of `op-reth`.

#### Running the Odyssey execution node

To run Odyssey from source, clone the repository and run the following commands:

```bash
git clone https://github.com/ithacaxyz/odyssey.git
cd odyssey
cargo install --path bin/odyssey
odyssey node
    --chain etc/odyssey-genesis.json \
    --rollup.sequencer-http <TODO> \
    --http \
    --ws \
    --authrpc.port 9551 \
    --authrpc.jwtsecret /path/to/jwt.hex
```

#### Running op-node with the Odyssey configuration

Once `odyssey` is started, [`op-node`](https://github.com/ethereum-optimism/optimism/tree/develop/op-node) can be run with the
included `odyssey-rollup.json`:

```bash
cd odyssey/
op-node \
    --rollup.config ./etc/odyssey-rollup.json \
    --l1=<your-sepolia-L1-rpc> \
    --l2=http://localhost:9551 \
    --l2.jwt-secret=/path/to/jwt.hex \
    --rpc.addr=0.0.0.0 \
    --rpc.port=7000 \
    --l1.trustrpc
```

### Running Odyssey with Kurtosis

Running a local network with a full Odyssey OP stack with Kurtosis requires some extra setup, since Odyssey uses a forked version of `op-node`.

To get started, follow [these instructions](https://docs.kurtosis.com/install/) to install Kurtosis.

Next, clone and build the modified `optimism-contract-deployer` image:

```bash
git clone git@github.com:paradigmxyz/optimism-package.git
cd optimism-package
git switch odyssey
docker build . -t ethpandaops/optimism-contract-deployer:latest --progress plain
```

> [!NOTE]
>
> The image may fail to build if you have not allocated enough memory for Docker.

Finally, run start a Kurtosis enclave (ensure you are still in `optimism-package`):

```bash
kurtosis run --enclave op-devnet github.com/paradigmxyz/optimism-package@odyssey \
  --args-file https://raw.githubusercontent.com/ithacaxyz/odyssey/main/etc/kurtosis.yaml
```

This will start an enclave named `op-devnet`. You can tear down the enclave with `kurtosis enclave rm --force op-devnet`, or tear down all enclaves using `kurtosis clean -a`.

> [!NOTE]
>
> If you want to use a custom build of Odyssey, simply build an Odyssey image with `docker build . -t ghcr.io/ithacaxyz/odyssey:latest`.

Consult the [Kurtosis OP package](https://github.com/ethpandaops/optimism-package) repository for instructions on how to adjust the args file to spin up additional services, like a block exporer.

### Wallet extension

Odyssey has a custom `wallet_` namespace, that allows users to delegate their EOAs to a contract using EIP-7702, and perform transactions on those accounts, all funded by the sequencer.

To enable this namespace, set the environment variable `EXP1_SK` to a private key that will sign the transactions, and `EXP1_WHITELIST` to a comma-delimited list of checksummed addresses accounts are allowed to delegate to. The new RPC method, `wallet_sendTransaction`, will only sign transactions that either:

1. Delegate accounts to one of the whitelisted addresses using EIP-7702, or
1. Send transactions to an EIP-7702 EOA that is already delegated to a whitelisted address

The `wallet_sendTransaction` endpoint accepts the same fields as `eth_sendTransaction`, with these notable exceptions:

1. `nonce` must not be set, as this is managed by the node
1. `value` must be unset or 0
1. `from` must not be specified

The following fields are ignored, as they are overwritten internally:

1. `gasPrice` (and EIP-1559 gas related pricing fields)
1. `gasLimit`
1. `chainId`

To get the list of contracts that are whitelisted for `wallet_sendTransaction`, you can query `wallet_getCapabilities`.

### Security

See [SECURITY.md](SECURITY.md).

#### License

<sup>
Licensed under either of <a href="LICENSE-APACHE">Apache License, Version
2.0</a> or <a href="LICENSE-MIT">MIT license</a> at your option.
</sup>

<br>

<sub>
Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in these crates by you, as defined in the Apache-2.0 license,
shall be dual licensed as above, without any additional terms or conditions.
</sub>

<!-- [crates-badge]: https://img.shields.io/crates/v/odyssey.svg -->
<!-- [crates-io]: https://crates.io/crates/odyssey -->
<!-- [downloads-badge]: https://img.shields.io/crates/d/odyssey -->
[mit-badge]: https://img.shields.io/badge/license-MIT-blue.svg
[apache-badge]: https://img.shields.io/badge/license-Apache--2.0-blue.svg
[mit-url]: LICENSE-MIT
[apache-url]: LICENSE-APACHE
[actions-badge]: https://github.com/ithacaxyz/odyssey/workflows/unit/badge.svg
[actions-url]: https://github.com/ithacaxyz/odyssey/actions?query=workflow%3ACI+branch%3Amain
[foundry-odyssey]: https://github.com/ithacaxyz/foundry-odyssey
