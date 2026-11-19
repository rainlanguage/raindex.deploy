# Raindex SDK

A TypeScript/JavaScript SDK for interacting with Raindex contracts, providing
comprehensive functionality for order management, configuration parsing, and
blockchain interactions.

## What is Raindex?

Raindex is an **onchain contract** that enables users to deploy complex,
perpetual trading algorithms using **Rainlang**, a domain-specific language
interpreted onchain. Learn more about Rainlang in the
[official documentation](https://docs.rainlang.xyz/intro).

### How It Works

- **Dynamic Orders**: Unlike traditional order books, Raindex orders contain
  algorithms that determine token movements based on real-time conditions
- **Vault System**: Users deposit tokens into vaults (virtual accounts) instead
  of using token approvals
- **Multi-Token Strategies**: Orders can reference multiple input/output vaults
  for sophisticated trading scenarios
- **Perpetual Execution**: Strategies remain active until explicitly removed by
  the owner
- **Decentralized Execution**: Third-party fillers execute trades by
  capitalizing on arbitrage opportunities

## SDK Overview

This SDK provides Rust-powered WebAssembly bindings for Raindex functionality,
enabling developers to:

- **Query Orders & Trades**: Search orders across multiple networks, fetch order
  details, and track trade history
- **Execute Quotes**: Get real-time quotes for trading pairs with maximum output
  amounts and IO ratios
- **Take Orders**: Generate calldata for executing trades against orders, with
  auto-discovery by token pair or targeting specific orders
- **Manage Vaults**: Query vault balances, generate deposit/withdraw calldata,
  and track vault activity
- **Parse Configurations**: Validate YAML files defining networks, tokens,
  raindexes, and subgraph endpoints
- **Generate Transactions**: Create ABI-encoded calldata for adding/removing
  orders and vault operations
- **Track Performance**: Monitor order volume, vault balance changes, and
  trading metrics over time

## Prerequisites

Before using this SDK, ensure you have:

- Node.js >= 22
- A Web3 provider (e.g., ethers.js, viem)
- A YAML configuration file (see
  [example configuration](https://github.com/rainlanguage/rain.strategies/blob/main/settings.yaml))

## Installation

```bash
npm install @rainlanguage/raindex
```

## Quick Start

### Example configuration used in this guide

All of the code snippets below reuse the same fixed-limit dotrain/settings
source. The portion before `---` represents the shared raindex and dotrain YAML,
and Rainlang lives after the separator.

> **Heads-up:** These values are purely illustrative. Before deploying anything,
> pull the canonical strategies and settings from
> [rainlanguage/rain.strategies](https://github.com/rainlanguage/rain.strategies)
> to mirror what our web apps run in production.

```ts
const FIXED_LIMIT_SOURCE = `
version: 6

networks:
  base:
    rpcs:
      - https://base-rpc.publicnode.com
    chain-id: 8453
    network-id: 8453
    currency: ETH

metaboards:
  base: https://subgraph.api.ormilabs.com/api/public/9f4fc2fa-4a15-44f7-a7c1-66fdaa518a71/subgraphs/metadata-base/prod/gn

subgraphs:
  base: https://example.com/subgraph

local-db-remotes:
  raindex: https://example.com/subgraph

raindexes:
  base:
    network: base
    address: 0x52CEB8eBEf648744fFDDE89F7Bc9C3aC35944775
    deployment-block: 36667253
    subgraph: base
    local-db-remote: raindex

tokens:
  usdc:
    network: base
    address: 0xA0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48
    decimals: 6
    label: USD Coin
    symbol: USDC
  weth:
    network: base
    address: 0x4200000000000000000000000000000000000006
    decimals: 18
    label: Wrapped Ether
    symbol: WETH

rainlangs:
  base:
    network: base
    address: 0x6557778Db274f04B9E9f39F8Ff2D621c2036e978

orders:
  fixed-limit:
    raindex: base
    inputs:
      - token: usdc
        vault-id: 1
    outputs:
      - token: weth
        vault-id: 1
    rainlang: base

scenarios:
  base:
    raindex: base
    runs: 1
    bindings:
      raindex-subparser: 0x22839F16281E67E5Fd395fAFd1571e820CbD46cB
      fixed-io-output-token: 0x4200000000000000000000000000000000000006

deployments:
  base:
    order: fixed-limit
    scenario: base

using-tokens-from:
  - https://tokens.coingecko.com/base/all.json

builder:
  name: Fixed limit
  description: Deploy a USDC -> WETH limit order on Base.
  short-description: Deploy a USDC -> WETH limit order on Base.
  deployments:
    base:
      name: Base
      description: Deploy a limit order on Base.
      deposits:
        - token: usdc
          presets:
            - "0"
            - "100"
            - "1000"
      fields:
        - binding: fixed-io
          name: USDC per WETH
          description: Fixed exchange rate (USDC received per 1 WETH sold)
          presets:
            - value: "1800"
            - value: "1850"
            - value: "1900"
        - binding: amount-per-trade
          name: Amount per trade
          description: USDC spent per fill
          presets:
            - value: "100"
            - value: "250"
            - value: "500"
      select-tokens:
        - key: input-token
          name: Token to Buy
          description: Select the token you want to purchase
        - key: output-token
          name: Token to Sell
          description: Select the token you want to sell

---
#raindex-subparser !The subparser to use.

#fixed-io !The io ratio for the limit order.
#fixed-io-output-token !The output token that the fixed io is for. If this doesn't match the runtime output then the fixed-io will be inverted.

#calculate-io
using-words-from raindex-subparser
max-output: max-positive-value(),
io: if(
  equal-to(
    output-token()
    fixed-io-output-token
  )
  fixed-io
  inv(fixed-io)
);

#handle-io
:;

#handle-add-order
:;
`;

const RAINDEX_SETTINGS = FIXED_LIMIT_SOURCE.split("---")[0];
```

### 1. Create a raindex client

This first snippet does three things: (1) load one or more settings YAML strings
(these describe networks, accounts, and subgraph URLs), (2) feed those sources
into `RaindexClient.new` so the WASM layer can parse and validate them, and (3)
unwrap the resulting `WasmEncodedResult` so downstream samples can call the
client with standard JS error handling expectations. The constructor is
**async** — use `await`.

```ts
import { RaindexClient } from "@rainlanguage/raindex";

const clientResult = await RaindexClient.new([RAINDEX_SETTINGS]);
if (clientResult.error) throw new Error(clientResult.error.readableMsg);
const client = clientResult.value;
```

Pass `true` as the second argument to `RaindexClient.new` when you want strict
schema validation.

When the YAML includes `local-db-sync` sections, pass a local DB object to wire
up a local SQLite cache:

```ts
const clientResult = await RaindexClient.new([RAINDEX_SETTINGS], undefined, {
  localDb,
  statusCallback: updateStatusCallback,
});
```

The client will automatically start the sync scheduler and route queries to the
local DB for configured chains once the first sync cycle completes.

### 2. Query orders with filters & pagination

Here we scope the query by chain IDs and typical filters (owner, token, activity
flag), ask the client to hydrate matching orders, and then walk the richer
helpers on a single `RaindexOrder`—vault listings, trades, quotes, and detail
lookups—to show how pagination + follow-up queries hang together.

```ts
import type { ChainIds, GetOrdersFilters } from "@rainlanguage/raindex";

const chainIds: ChainIds = [8453];
const filters: GetOrdersFilters = {
  owners: ["0x1234..."],
  active: true,
  tokens: ["0xTokenAddress"],
};

const ordersResult = await client.getOrders(chainIds, filters, 1);
if (ordersResult.error) throw new Error(ordersResult.error.readableMsg);
const orders = ordersResult.value; // RaindexOrder[]

const first = orders[0];
const vaultList = first.vaultsList; // RaindexVaultsList helper
const rawVaults = vaultList.items; // RaindexVault[]

const tradesResult = await first.getTradesList();
if (tradesResult.error) throw new Error(tradesResult.error.readableMsg);

const tradeDetailResult = await first.getTradeDetail("0xTradeId");
if (tradeDetailResult.error) {
  throw new Error(tradeDetailResult.error.readableMsg);
}

const quotesResult = await first.getQuotes();
if (quotesResult.error) throw new Error(quotesResult.error.readableMsg);
```

Additional helpers worth wiring up:

- `client.getOrderByHash(chainId, raindexAddress, orderHash)` – fetch a single
  order with full vault metadata.
- `client.getAddOrdersForTransaction(...)` /
  `client.getRemoveOrdersForTransaction(...)` – diff deployments and removals by
  transaction hash.
- `client.getTransaction(raindexAddress, txHash)` – inspect who sent a
  transaction, the block number, and timestamp.

#### Poll for newly deployed orders

After you submit an `execute`/`addOrders` transaction you immediately have the
transaction hash, but the subgraph still needs a few blocks to index the
resulting order. Rather than re-querying every order and diffing manually, poll
`client.getAddOrdersForTransaction` with that hash until it returns at least one
`RaindexOrder`.

```ts
import type { RaindexClient } from '@rainlanguage/raindex';

const POLL_INTERVAL_MS = 5_000;
const MAX_ATTEMPTS = 12;

async function waitForOrderFromTx(
  client: RaindexClient,
  {
    chainId,
    raindexAddress,
    txHash
  }: {
    chainId: number;
    raindexAddress: string;
    txHash: string;
  }
) {
  for (let attempt = 0; attempt < MAX_ATTEMPTS; attempt++) {
    const result = await client.getAddOrdersForTransaction(chainId, raindexAddress, txHash);
    if (result.error) throw new Error(result.error.readableMsg);

    if (result.value.length) {
      return result.value[0]; // RaindexOrder; multiple orders are possible for batched deployments
    }
    await new Promise((resolve) => setTimeout(resolve, POLL_INTERVAL_MS));
  }
  throw new Error('Order not indexed yet; increase MAX_ATTEMPTS or interval if needed');
}

const txReceipt = await executeOrder(...);
const raindexOrder = await waitForOrderFromTx(client, {
  chainId: 8453,
  raindexAddress: '0x52CEB8eBEf648744fFDDE89F7Bc9C3aC35944775',
  txHash: txReceipt.transactionHash
});
```

#### Fetch a single order by hash

```ts
const orderResult = await client.getOrderByHash(
  8453, // Base
  "0x52CEB8eBEf648744fFDDE89F7Bc9C3aC35944775",
  "0x1234567890abcdef1234567890abcdef1234567890abcdef1234567890abcdef12",
);
if (orderResult.error) throw new Error(orderResult.error.readableMsg);
const order = orderResult.value; // RaindexOrder

const vaultsList = order.vaultsList;
const removeCalldataResult = await order.getRemoveCalldata();
if (removeCalldataResult.error) {
  throw new Error(removeCalldataResult.error.readableMsg);
}
```

### 3. Work with vaults & Floats

Vault workflows usually require combining filters, inspecting the returned
`RaindexVaultsList`, and then producing calldata or math-heavy amounts. This
example chains those steps: fetch vaults, narrow the list to withdrawable
entries, pull history, parse human inputs with `Float`, and finally build
deposit/withdraw/approval payloads while checking allowances.

```ts
import { Float, type GetVaultsFilters } from "@rainlanguage/raindex";

const vaultFilters: GetVaultsFilters = {
  owners: ["0x1234..."],
  hideZeroBalance: true,
};

const vaultsResult = await client.getVaults([8453], vaultFilters, 1);
if (vaultsResult.error) throw new Error(vaultsResult.error.readableMsg);
const vaultsList = vaultsResult.value; // RaindexVaultsList

const withdrawableResult = vaultsList.getWithdrawableVaults();
if (withdrawableResult.error) {
  throw new Error(withdrawableResult.error.readableMsg);
}
const withdrawableVaults = withdrawableResult.value;

const vault = withdrawableVaults[0];
const historyResult = await vault.getBalanceChanges();
if (historyResult.error) throw new Error(historyResult.error.readableMsg);

const depositAmount = Float.parse("10.5");
if (depositAmount.error) throw new Error(depositAmount.error.readableMsg);
const depositCalldataResult = await vault.getDepositCalldata(
  depositAmount.value,
);
if (depositCalldataResult.error) {
  throw new Error(depositCalldataResult.error.readableMsg);
}

const withdrawAmount = Float.parse("2");
if (withdrawAmount.error) throw new Error(withdrawAmount.error.readableMsg);
const withdrawCalldataResult = await vault.getWithdrawCalldata(
  withdrawAmount.value,
);
if (withdrawCalldataResult.error) {
  throw new Error(withdrawCalldataResult.error.readableMsg);
}

const approvalResult = await vault.getApprovalCalldata(depositAmount.value);
if (approvalResult.error) throw new Error(approvalResult.error.readableMsg);

const allowanceResult = await vault.getAllowance();
if (allowanceResult.error) throw new Error(allowanceResult.error.readableMsg);
const allowance = allowanceResult.value;
```

`RaindexVaultsList` also exposes `getWithdrawCalldata()` (builds a multicall to
empty every vault with a balance), `pickByIds([...])`, and `concat(otherList)`
if you need to restructure vault groups before submitting a transaction.

#### Fetch a single vault

```ts
const vaultResult = await client.getVault(
  8453,
  "0x52CEB8eBEf648744fFDDE89F7Bc9C3aC35944775",
  "0x01",
);
if (vaultResult.error) throw new Error(vaultResult.error.readableMsg);
const vault = vaultResult.value; // RaindexVault

const balanceChangesResult = await vault.getBalanceChanges();
if (balanceChangesResult.error) {
  throw new Error(balanceChangesResult.error.readableMsg);
}
```

### 4. Generate quotes & calldata

Once you have hydrated orders, you typically need deterministic hashes plus
calldata builders. The snippet below hashes an order struct, generates
take-orders calldata, asks an order for its removal calldata, and fetches
quotes—mirroring the usual "inspect -> prepare transaction -> submit" flow.

```ts
import { getOrderHash, getTakeOrders5Calldata } from "@rainlanguage/raindex";

const orderHashResult = getOrderHash(orderV4Struct);
if (orderHashResult.error) throw new Error(orderHashResult.error.readableMsg);

const takeOrdersResult = getTakeOrders5Calldata(takeOrdersConfig);
if (takeOrdersResult.error) throw new Error(takeOrdersResult.error.readableMsg);
const takeOrdersCalldata = takeOrdersResult.value; // hex string ready for the contract

const removeCalldataResult = await first.getRemoveCalldata();
if (removeCalldataResult.error) {
  throw new Error(removeCalldataResult.error.readableMsg);
}

const quotesResult = await first.getQuotes();
if (quotesResult.error) throw new Error(quotesResult.error.readableMsg);
```

Every `RaindexOrder` exposes `vaultsList`, `inputsList`, `outputsList`, and
`inputsOutputsList`, so you can quickly scope which vault IDs map to which IO
leg before building calldata.

### 5. Take orders

The SDK provides two approaches for executing `takeOrders5` transactions:
auto-discovery by token pair or targeting a specific known order.

#### Take orders by token pair (auto-discovery)

Use `client.getTakeOrdersCalldata()` to discover and aggregate liquidity across
all active orders for a given token pair:

```ts
import type { TakeOrdersRequest } from "@rainlanguage/raindex";

const request: TakeOrdersRequest = {
  chainId: 137,
  taker: "0xYourAddress...",
  sellToken: "0xUSDC...", // Token you will GIVE
  buyToken: "0xWETH...", // Token you will RECEIVE
  mode: "BuyUpTo", // BuyExact | BuyUpTo | SpendExact | SpendUpTo
  amount: "10", // Target amount (buy tokens for buy modes, sell tokens for spend modes)
  priceCap: "1.2", // Maximum price (sell per 1 buy)
};

const takeResult = await client.getTakeOrdersCalldata(request);
if (takeResult.error) throw new Error(takeResult.error.readableMsg);

const {
  raindex, // Contract address to call
  calldata, // ABI-encoded takeOrders5 calldata
  effectivePrice, // Blended price from simulation
  prices, // Per-leg ratios (best to worst)
  expectedSell, // Simulated sell amount at current quotes
  maxSellCap, // Worst-case spend cap
} = takeResult.value;
```

**Take order modes:**

- `BuyExact` – Buy exactly `amount` of buy token (reverts if insufficient
  liquidity)
- `BuyUpTo` – Buy up to `amount` of buy token (partial fills allowed)
- `SpendExact` – Spend exactly `amount` of sell token (reverts if insufficient
  liquidity)
- `SpendUpTo` – Spend up to `amount` of sell token (partial fills allowed)

#### Take a specific order

When you already have a `RaindexOrder` instance, use `order.getTakeCalldata()`
to target that specific order:

```ts
const order = orders[0];

const takeResult = await order.getTakeCalldata(
  0, // inputIndex - index in order's validInputs array
  0, // outputIndex - index in order's validOutputs array
  "0xTaker...", // taker address
  "BuyUpTo", // mode
  "10", // amount
  "1.2", // priceCap
);
if (takeResult.error) throw new Error(takeResult.error.readableMsg);

const { calldata, raindex, effectivePrice, expectedSell, maxSellCap } =
  takeResult.value;
```

#### Estimate take order amounts

Before executing, estimate what you'll spend/receive for a given amount:

```ts
const quotesResult = await order.getQuotes();
if (quotesResult.error) throw new Error(quotesResult.error.readableMsg);
const quote = quotesResult.value[0]; // Pick the quote for your desired pair

const estimateResult = order.estimateTakeOrder(
  quote,
  true, // isBuy - true for buying output token, false for selling input token
  "10", // amount as decimal string
);
if (estimateResult.error) throw new Error(estimateResult.error.readableMsg);

const {
  expectedSpend, // How much sell token you'll give
  expectedReceive, // How much buy token you'll get
  isPartial, // True if order can't fully fill your amount
} = estimateResult.value;
```

#### Filter orders by input/output tokens

Use directional token filters to find orders matching specific trading pairs:

```ts
import type {
  GetOrdersFilters,
  GetOrdersTokenFilter,
} from "@rainlanguage/raindex";

const tokenFilter: GetOrdersTokenFilter = {
  inputs: ["0xUSDC..."], // Orders that accept USDC as input
  outputs: ["0xWETH..."], // Orders that output WETH
};

const filters: GetOrdersFilters = {
  tokens: tokenFilter,
  active: true,
};

const ordersResult = await client.getOrders([137], filters, 1);
```

## Dotrain authoring & deployment flows

### Load remote strategies with `DotrainRegistry`

If you maintain a hosted registry, instantiate the helper, inspect what it
exposes, and pull down any dotrain/builder definitions you need:

```ts
import { DotrainRegistry } from "@rainlanguage/raindex";

const registryResult = await DotrainRegistry.new(
  "https://example.com/registry.txt",
);
if (registryResult.error) throw new Error(registryResult.error.readableMsg);
const registry = registryResult.value;

const orderMenuResult = registry.getAllOrderDetails();
if (orderMenuResult.error) throw new Error(orderMenuResult.error.readableMsg);
const orderMenu = orderMenuResult.value.valid; // Map<orderKey, { name, description, short_description }>
const invalidOrders = orderMenuResult.value.invalid; // Map<orderKey, WasmEncodedError>

const deploymentsResult = registry.getDeploymentDetails("fixed-limit");
if (deploymentsResult.error) {
  throw new Error(deploymentsResult.error.readableMsg);
}
const deployments = deploymentsResult.value;
```

Registry manifests follow the format:

```
https://example.com/shared-settings.yaml
fixed-limit https://example.com/orders/fixed-limit.rain
dca https://example.com/orders/dca.rain
```

The SDK merges the shared settings YAML with each order's `.rain` content before
you ever build an order builder.

#### Access tokens from registry settings

Use `getRaindexYaml()` to access the shared settings as an `RaindexYaml`
instance, then query tokens, networks, or raindexes:

```ts
const raindexYamlResult = registry.getRaindexYaml();
if (raindexYamlResult.error) {
  throw new Error(raindexYamlResult.error.readableMsg);
}
const raindexYaml = raindexYamlResult.value;

const tokensResult = await raindexYaml.getTokens();
if (tokensResult.error) throw new Error(tokensResult.error.readableMsg);
const tokens = tokensResult.value; // TokenInfo[] with chain_id, address, decimals, symbol, name
```

#### Get a RaindexClient from registry settings

Use `getRaindexClient()` to create a `RaindexClient` directly from the
registry's shared settings, without manually bridging through `RaindexYaml`:

```ts
const clientResult = registry.getRaindexClient();
if (clientResult.error) throw new Error(clientResult.error.readableMsg);
const client = clientResult.value;

// Use the client to query orders, vaults, etc.
const ordersResult = await client.getOrders([8453]);
```

### Build a deployment order builder

Any dotrain file that includes a `builder:` block plus the usual settings YAML
is enough to drive `RaindexOrderBuilder`. The `FIXED_LIMIT_SOURCE` constant
declared earlier already includes the required networks/tokens/deployers plus a
full `builder` definition, so you can reference it directly (or trim it to your
own bindings) instead of copying pieces of `settings.yaml` inline in this guide.
Always cross-check the source you feed in with the latest definitions in
[rainlanguage/rain.strategies](https://github.com/rainlanguage/rain.strategies);
that repository tracks the real configurations our UI ships with.

With that single source string (read from disk or built dynamically) you can
drive the full order builder workflow:

```ts
import { RaindexOrderBuilder } from "@rainlanguage/raindex";

const dotrainWithBuilder = FIXED_LIMIT_SOURCE;
const SAMPLE_YAML = `
...
rainlangs:
    rainlang1:
        network: mainnet
        address: 0x...
raindexes:
    raindex1:
        address: 0x...
        network: mainnet
...
`;
const additionalSettings = [SAMPLE_YAML]; // optional extra YAML strings

const deploymentsResult = await RaindexOrderBuilder.getDeploymentKeys(
  dotrainWithBuilder,
  additionalSettings,
);
if (deploymentsResult.error) {
  throw new Error(deploymentsResult.error.readableMsg);
}
const [firstDeployment] = deploymentsResult.value;

const builderResult = await RaindexOrderBuilder.newWithDeployment(
  dotrainWithBuilder,
  additionalSettings,
  firstDeployment,
);
if (builderResult.error) throw new Error(builderResult.error.readableMsg);
const builder = builderResult.value;

const configResult = builder.getAllBuilderConfig();
if (configResult.error) throw new Error(configResult.error.readableMsg);
const config = configResult.value;

const selectTokensResult = builder.getSelectTokens();
if (selectTokensResult.error) {
  throw new Error(selectTokensResult.error.readableMsg);
}
const selectTokens = selectTokensResult.value;

const depositsResult = builder.getDeposits();
if (depositsResult.error) throw new Error(depositsResult.error.readableMsg);
const deposits = depositsResult.value;

await builder.setSelectToken(
  "input-token",
  "0xA0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48",
); // USDC
await builder.setSelectToken(
  "output-token",
  "0x4200000000000000000000000000000000000006",
); // WETH
const fieldResult = builder.setFieldValue("fixed-io", "1850");
if (fieldResult.error) throw new Error(fieldResult.error.readableMsg);
const amountResult = builder.setFieldValue("amount-per-trade", "250");
if (amountResult.error) throw new Error(amountResult.error.readableMsg);
await builder.setDeposit("usdc", "5000");
const vaultIdResult = builder.setVaultId("input", "usdc", "42");
if (vaultIdResult.error) throw new Error(vaultIdResult.error.readableMsg);

const allowancesResult = await builder.checkAllowances("0xOwner");
if (allowancesResult.error) throw new Error(allowancesResult.error.readableMsg);
const allowances = allowancesResult.value;

const approvalCalldatasResult = await builder.generateApprovalCalldatas(
  "0xOwner",
);
if (approvalCalldatasResult.error) {
  throw new Error(approvalCalldatasResult.error.readableMsg);
}

const depositCalldatasResult = await builder.generateDepositCalldatas();
if (depositCalldatasResult.error) {
  throw new Error(depositCalldatasResult.error.readableMsg);
}

const deploymentArgsResult = await builder.getDeploymentTransactionArgs(
  "0xOwner",
);
if (deploymentArgsResult.error) {
  throw new Error(deploymentArgsResult.error.readableMsg);
}
const { approvals, deploymentCalldata, raindexAddress, chainId } =
  deploymentArgsResult.value;

const rainlangResult = await builder.getComposedRainlang();
if (rainlangResult.error) throw new Error(rainlangResult.error.readableMsg);
const composedRainlang = rainlangResult.value;

const serializedStateResult = builder.serializeState();
if (!serializedStateResult.error) {
  localStorage.setItem("fixed-limit-state", serializedStateResult.value);
}
```

Serialize the builder state and later revive it with
`RaindexOrderBuilder.newFromState(dotrainText, additionalSettings, serializedState, callback)`
if you want to skip re-entering form choices.

#### Deploy with wallet + fetch your order

`builder.getDeploymentTransactionArgs(owner)` returns a
`DeploymentTransactionArgs` struct with:

- `approvals: ExtendedApprovalCalldata[]` (each item contains `token`,
  `calldata`, and the token `symbol` for UX)
- `deploymentCalldata: Hex` – a multicall that performs deposits (if required)
  and adds the order in one transaction
- `raindexAddress: string` – destination for the multicall
- `chainId: number` – network you must connect your wallet to

A typical deployment flow is:

1. Run `getDeploymentTransactionArgs`
2. Submit every approval in series (skip when `approvals` is empty)
3. Submit the deployment calldata
4. Poll `client.getAddOrdersForTransaction` (the helper shown earlier) with the
   deployment hash until the subgraph surfaces your `RaindexOrder`

```ts
import type { RaindexClient } from "@rainlanguage/raindex";

const deploymentArgsResult = await builder.getDeploymentTransactionArgs(owner);
if (deploymentArgsResult.error) {
  throw new Error(deploymentArgsResult.error.readableMsg);
}
const { approvals, deploymentCalldata, raindexAddress, chainId } =
  deploymentArgsResult.value;

// Assume sendTransaction({ to, data }) and waitForReceipt(hash) come from your wallet stack.
for (const approval of approvals) {
  const approvalHash = await sendTransaction({
    to: approval.token,
    data: approval.calldata,
  });
  await waitForReceipt(approvalHash);
}

const deploymentHash = await sendTransaction({
  to: raindexAddress,
  data: deploymentCalldata,
});
await waitForReceipt(deploymentHash);

const raindexOrder = await waitForOrderFromTx(client as RaindexClient, {
  chainId,
  raindexAddress,
  txHash: deploymentHash,
});
```

After you have a local dotrain source, you can also fetch equivalent sources
from a registry and run the same flow:

```ts
import { DotrainRegistry } from "@rainlanguage/raindex";

const registryResult = await DotrainRegistry.new(
  "https://example.com/registry.txt",
);
if (registryResult.error) throw new Error(registryResult.error.readableMsg);
const registry = registryResult.value;

const builderResult = await registry.getOrderBuilder("fixed-limit", "base");
if (builderResult.error) throw new Error(builderResult.error.readableMsg);
const builderFromRegistry = builderResult.value;

// builderFromRegistry is already a RaindexOrderBuilder instance, so you can reuse
// the same builder helper steps shown above (select tokens, deposits, calldata, etc.).
```

### Work directly with dotrain files

If you just need Rainlang composition (no builder state), read the dotrain text
plus shared settings yourself, instantiate a `DotrainOrder`, and then ask it to
compose scenario/deployment/post-task Rainlang. The example below reuses
`FIXED_LIMIT_SOURCE`, but you can replace it with the contents of any `.rain`
file.

```ts
import { DotrainOrder } from "@rainlanguage/raindex";

const dotrainResult = await DotrainOrder.create(FIXED_LIMIT_SOURCE, [
  RAINDEX_SETTINGS,
]);
if (dotrainResult.error) throw new Error(dotrainResult.error.readableMsg);
const dotrain = dotrainResult.value;

const scenarioResult = await dotrain.composeScenarioToRainlang("backtest");
if (!scenarioResult.error) console.log(scenarioResult.value);

const deploymentResult = await dotrain.composeDeploymentToRainlang(
  "flare-prod",
);
if (!deploymentResult.error) console.log(deploymentResult.value);

const postTaskResult = await dotrain.composeScenarioToPostTaskRainlang(
  "flare-prod",
);
if (!postTaskResult.error) console.log(postTaskResult.value);
```

## Utility exports

- `getOrderHash`, `keccak256`, `keccak256HexString` – deterministic hashing
  helpers for Rain orders or arbitrary payloads.
- `Float` – arbitrary-precision arithmetic with parsing, formatting,
  comparisons, math ops, fixed-decimal conversions, and helpers like
  `Float.zero()` or `.formatWithRange(...)`.
- `RaindexYaml.getTokens()` – async method returning all tokens from YAML
  configuration with `chain_id`, `address`, `decimals`, `symbol`, and `name`.
  Automatically fetches remote tokens from `using-tokens-from` URLs.
- `RaindexClient.getAllAccounts()` / `getAllVaultTokens()` – introspect accounts
  and ERC20 metadata defined in your YAML or discovered via subgraphs.
- Local DB sync – pass `{ localDb, statusCallback }` to `RaindexClient.new()`
  when YAML has `local-db-sync` sections to enable an offline-capable persistent
  cache. The scheduler starts automatically and queries route to the local DB
  once the first sync cycle completes.
- `RaindexVaultsList.getWithdrawCalldata()` – multicall builder that withdraws
  every vault with a balance.
- `RaindexOrder.convertToSgOrder()` – convert WASM order representations back
  into the raw subgraph schema when you need to interop with other tooling.
- `TakeOrdersRequest`, `TakeOrdersCalldataResult`, `TakeOrderEstimate` – types
  for the take orders API.
- `GetOrdersTokenFilter` – directional token filter with `inputs` and `outputs`
  arrays for precise order discovery.

## Error handling pattern

Every exported function returns a `WasmEncodedResult<T>`:

```ts
type WasmEncodedResult<T> =
  | { value: T; error: undefined }
  | { value: undefined; error: { msg: string; readableMsg: string } };

const result = await client.getVault(14, "0xRaindex", "0x01");
if (result.error) {
  console.error("Vault lookup failed:", result.error.readableMsg);
  return;
}
console.log(result.value);
```

## Contributing

This SDK is part of the Rain Language ecosystem. For contributions and issues,
please visit the
[GitHub repository](https://github.com/rainlanguage/raindex.deploy).
