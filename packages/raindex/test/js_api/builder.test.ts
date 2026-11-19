import { decodeFunctionData, hexToBytes } from "viem";
import assert from "assert";
import {
  afterAll,
  beforeAll,
  beforeEach,
  describe,
  expect,
  it,
  Mock,
  vi,
} from "vitest";
import {
  RaindexOrderBuilder,
  ApprovalCalldataResult,
  DeploymentTransactionArgs,
  DepositCalldataResult,
  OrderBuilderCfg,
  OrderBuilderDeploymentCfg,
  OrderBuilderFieldDefinitionCfg,
  OrderBuilderPresetCfg,
  OrderBuilderSelectTokensCfg,
  NameAndDescriptionCfg,
  TokenAllowance,
  TokenDeposit,
  TokenInfo,
  AllBuilderConfig,
  WasmEncodedResult,
  FieldValue,
  RaindexYaml,
} from "../../dist/cjs";

const SPEC_VERSION = RaindexYaml.getCurrentSpecVersion().value;
import { getLocal } from "mockttp";

const builderConfig = `
builder:
  name: Fixed limit
  description: Fixed limit order
  short-description: Buy WETH with USDC on Base.
  deployments:
    some-deployment:
      name: Buy WETH with USDC on Base.
      description: Buy WETH with USDC for fixed price on Base network.
      short-description: Buy WETH with USDC on Base.
      deposits:
        - token: token1
          min: 0
          presets:
            - "0"
            - "10"
            - "100"
            - "1000"
            - "10000"
      fields:
        - binding: binding-1
          name: Field 1 name
          description: Field 1 description
          presets:
            - name: Preset 1
              value: "0x1234567890abcdef1234567890abcdef12345678"
            - name: Preset 2
              value: "false"
            - name: Preset 3
              value: "some-string"
          default: some-default-value
        - binding: binding-2
          name: Field 2 name
          description: Field 2 description
          min: 100
          presets:
            - value: "99.2"
            - value: "582.1"
            - value: "648.239"
          show-custom-field: true
    other-deployment:
      name: Test test
      description: Test test test
      deposits:
        - token: token1
          min: 0
          presets:
            - "0"
      fields:
        - binding: binding-1
          name: Field 1 name
          description: Field 1 description
          presets:
            - name: Preset 1
              value: "0"
        - binding: binding-2
          name: Field 2 name
          description: Field 2 description
          min: 100
          presets:
            - value: "0"
`;
const builderConfig2 = `
builder:
  name: Test test
  description: Test test test
  deployments:
    other-deployment:
      name: Test test
      description: Test test test
      deposits:
        - token: token1
          min: 0
          presets:
            - "0"
        - token: token2
          min: 0
          presets:
            - "0"
      fields:
        - binding: test-binding
          name: Test binding
          description: Test binding description
          presets:
            - value: "0xbeef"
          default: 10
`;
const builderConfig3 = `
builder:
  name: Test test
  description: Test test test
  deployments:
    other-deployment:
      name: Test test
      description: Test test test
      deposits:
        - token: token1
          min: 0
          presets:
            - "0"
        - token: token2
          min: 0
          presets:
            - "0"
      fields:
        - binding: test-binding
          name: Test binding
          description: Test binding description
          presets:
            - value: "test-value"
      select-tokens:
        - key: token1
        - key: token2
`;

const dotrain = `
version: ${SPEC_VERSION}
networks:
    some-network:
        rpcs:
            - http://localhost:8085/rpc-url
        chain-id: 123
        network-id: 123
        currency: ETH

subgraphs:
    some-sg: https://www.some-sg.com
metaboards:
    test: http://localhost:8085/metaboard
    some-network: http://localhost:8085/metaboard

rainlangs:
    some-deployer:
        network: some-network
        address: 0xF14E09601A47552De6aBd3A0B165607FaFd2B5Ba

raindexes:
    some-raindex:
        address: 0xc95A5f8eFe14d7a20BD2E5BAFEC4E71f8Ce0B9A6
        network: some-network
        subgraph: some-sg
        deployment-block: 12345

tokens:
    token1:
        network: some-network
        address: 0xc2132d05d31c914a87c6611c10748aeb04b58e8f
        decimals: 6
        label: Token 1
        symbol: T1
    token2:
        network: some-network
        address: 0x8f3cf7ad23cd3cadbd9735aff958023239c6a063
        decimals: 18
        label: Token 2
        symbol: T2

scenarios:
    some-scenario:
        rainlang: some-deployer
        bindings:
            test-binding: 5
        scenarios:
            sub-scenario:
                bindings:
                    another-binding: 300

orders:
    some-order:
      inputs:
        - token: token1
          vault-id: 1
      outputs:
        - token: token2
          vault-id: 1
      rainlang: some-deployer
      raindex: some-raindex

deployments:
    some-deployment:
        scenario: some-scenario
        order: some-order
    other-deployment:
        scenario: some-scenario.sub-scenario
        order: some-order
---
#test-binding !
#another-binding !
#calculate-io
_ _: 0 0;
#handle-io
:;
#handle-add-order
:;
`;
const dotrainWithoutVaultIds = `
version: ${SPEC_VERSION}
networks:
    some-network:
        rpcs:
            - http://localhost:8085/rpc-url
        chain-id: 123
        network-id: 123
        currency: ETH

subgraphs:
    some-sg: https://www.some-sg.com
metaboards:
    test: http://localhost:8085/metaboard
    some-network: http://localhost:8085/metaboard

rainlangs:
    some-deployer:
        network: some-network
        address: 0xF14E09601A47552De6aBd3A0B165607FaFd2B5Ba

raindexes:
    some-raindex:
        address: 0xc95A5f8eFe14d7a20BD2E5BAFEC4E71f8Ce0B9A6
        network: some-network
        subgraph: some-sg
        deployment-block: 12345

tokens:
    token1:
        network: some-network
        address: 0xc2132d05d31c914a87c6611c10748aeb04b58e8f
        decimals: 6
        label: Token 1
        symbol: T1
    token2:
        network: some-network
        address: 0x8f3cf7ad23cd3cadbd9735aff958023239c6a063
        decimals: 18
        label: Token 2
        symbol: T2

scenarios:
    some-scenario:
        rainlang: some-deployer
        bindings:
            test-binding: 5

orders:
    some-order:
      inputs:
        - token: token1
      outputs:
        - token: token2
      rainlang: some-deployer
      raindex: some-raindex

deployments:
    some-deployment:
        scenario: some-scenario
        order: some-order
    other-deployment:
        scenario: some-scenario
        order: some-order
---
#test-binding !
#calculate-io
_ _: 0 0;
#handle-io
:;
#handle-add-order
:;
`;
const dotrainWithoutTokens = `
version: ${SPEC_VERSION}
networks:
    some-network:
        rpcs:
            - http://localhost:8085/rpc-url
        chain-id: 123
        network-id: 123
        currency: ETH

subgraphs:
    some-sg: https://www.some-sg.com
metaboards:
    test: http://localhost:8085/metaboard

rainlangs:
    some-deployer:
        network: some-network
        address: 0xF14E09601A47552De6aBd3A0B165607FaFd2B5Ba

raindexes:
    some-raindex:
        address: 0xc95A5f8eFe14d7a20BD2E5BAFEC4E71f8Ce0B9A6
        network: some-network
        subgraph: some-sg
        deployment-block: 12345

scenarios:
    some-scenario:
        rainlang: some-deployer
        bindings:
            test-binding: 5

orders:
    some-order:
      inputs:
        - token: token1
      outputs:
        - token: token2
      rainlang: some-deployer
      raindex: some-raindex

deployments:
    some-deployment:
        scenario: some-scenario
        order: some-order
    other-deployment:
        scenario: some-scenario
        order: some-order
---
#calculate-io
_ _: 0 0;
#handle-io
:;
#handle-add-order
:;
`;
const dotrainWithTokensMismatch = dotrain.replace(
  "0xc95A5f8eFe14d7a20BD2E5BAFEC4E71f8Ce0B9A6",
  "0xc95A5f8eFe14d7a20BD2E5BAFEC4E71f8Ce0B9A7",
);
const dotrainForRemotes = `
version: ${SPEC_VERSION}
builder:
  name: Test
  description: Fixed limit order
  deployments:
    test-deployment:
      name: Test deployment
      description: Test description
      deposits:
        - token: token1
        - token: token2
      fields:
        - binding: binding-1
          name: Field 1 name
          default: some-default-value
    other-deployment:
      name: Test deployment
      description: Test description
      deposits:
        - token: token3
      fields:
        - binding: binding-1
          name: Field 1 name
          default: some-default-value
networks:
    some-network:
        rpcs:
            - http://localhost:8085/rpc-url
        chain-id: 999
        network-id: 999
        currency: ZZ
using-networks-from:
  chainid:
    url: http://localhost:8085/remote-networks
    format: chainid
subgraphs:
    some-sg: https://www.some-sg.com
    other-sg: https://www.other-sg.com
metaboards:
    test: http://localhost:8085/metaboard
    some-network: http://localhost:8085/metaboard
rainlangs:
    some-deployer:
        network: remote-network
        address: 0xF14E09601A47552De6aBd3A0B165607FaFd2B5Ba
    other-deployer:
        network: some-network
        address: 0xF14E09601A47552De6aBd3A0B165607FaFd2B5Ba
raindexes:
    some-raindex:
        address: 0xc95A5f8eFe14d7a20BD2E5BAFEC4E71f8Ce0B9A6
        network: remote-network
        subgraph: some-sg
        local-db-remote: remote
        deployment-block: 12345
    other-raindex:
        address: 0xc95A5f8eFe14d7a20BD2E5BAFEC4E71f8Ce0B9A6
        network: some-network
        subgraph: other-sg
        local-db-remote: remote
        deployment-block: 12345
using-tokens-from:
  - http://localhost:8085/remote-tokens
tokens:
    token1:
        network: remote-network
        address: 0xc2132d05d31c914a87c6611c10748aeb04b58e8f
        decimals: 6
        label: Token 1
        symbol: T1
    token2:
        network: remote-network
        address: 0x8f3cf7ad23cd3cadbd9735aff958023239c6a063
        decimals: 18
        label: Token 2
        symbol: T2
    token3:
        network: some-network
        address: 0xadf0000000000000000000000000000000000000
        decimals: 6
        label: Token 3
        symbol: T3
scenarios:
    some-scenario:
        rainlang: some-deployer
    other-scenario:
        rainlang: other-deployer
orders:
    some-order:
      inputs:
        - token: token1
      outputs:
        - token: token2
      rainlang: some-deployer
      raindex: some-raindex
    other-order:
      inputs:
        - token: token3
      outputs:
        - token: token3
      rainlang: other-deployer
      raindex: other-raindex
deployments:
    test-deployment:
        scenario: some-scenario
        order: some-order
    other-deployment:
        scenario: other-scenario
        order: other-order
---
_: 10,
_: 20;
`;
const dotrainWithBuilder = `
${builderConfig}

${dotrain}
`;

describe("Rain Raindex JS API Package Bindgen Tests - Builder", async function () {
  const mockServer = getLocal();
  beforeAll(async () => {
    await mockServer.start(8085);
  });
  afterAll(async () => {
    await mockServer.stop();
  });
  beforeEach(() => {
    mockServer.reset();
  });

  const extractWasmEncodedData = <T>(
    result: WasmEncodedResult<T>,
    errorMessage?: string,
  ): T => {
    if (result.error) {
      assert.fail(errorMessage ?? result.error.msg);
    }

    if (result.value === undefined) {
      return result.value as T;
    }

    return result.value;
  };

  const mockRainlangRegistryCalls = async () => {
    await mockServer
      .forPost("/rpc-url")
      .withBodyIncluding("0x10762802")
      .thenSendJsonRpcResult(`0x${"0".repeat(24) + "1".repeat(40)}`);
    await mockServer
      .forPost("/rpc-url")
      .withBodyIncluding("0x4501e517")
      .thenSendJsonRpcResult(`0x${"0".repeat(24) + "2".repeat(40)}`);
    await mockServer
      .forPost("/rpc-url")
      .withBodyIncluding("0xf2c4da93")
      .thenSendJsonRpcResult(`0x${"0".repeat(24) + "3".repeat(40)}`);
    await mockServer
      .forPost("/rpc-url")
      .withBodyIncluding("0x0c1916a4")
      .thenSendJsonRpcResult(`0x${"0".repeat(24) + "3".repeat(40)}`);
  };

  const metaBoardAbi = [
    {
      type: "function",
      name: "emitMeta",
      inputs: [
        { name: "subject", type: "bytes32" },
        { name: "meta", type: "bytes" },
      ],
      outputs: [],
      stateMutability: "nonpayable",
    },
  ] as const;

  it("should return available deployments", async () => {
    const result =
      await RaindexOrderBuilder.getDeploymentKeys(dotrainWithBuilder);
    const deployments = extractWasmEncodedData<string[]>(result);
    assert.equal(deployments.length, 2);
    assert.ok(deployments.includes("some-deployment"));
    assert.ok(deployments.includes("other-deployment"));
  });

  it("should initialize builder object", async () => {
    // mock the rpc call to get token info
    mockServer
      .forPost("/rpc-url")
      .withBodyIncluding("0x82ad56cb")
      .thenSendJsonRpcResult(
        "0x00000000000000000000000000000000000000000000000000000000000000200000000000000000000000000000000000000000000000000000000000000003000000000000000000000000000000000000000000000000000000000000006000000000000000000000000000000000000000000000000000000000000000e000000000000000000000000000000000000000000000000000000000000001a0000000000000000000000000000000000000000000000000000000000000000100000000000000000000000000000000000000000000000000000000000000400000000000000000000000000000000000000000000000000000000000000020000000000000000000000000000000000000000000000000000000000000000600000000000000000000000000000000000000000000000000000000000000010000000000000000000000000000000000000000000000000000000000000040000000000000000000000000000000000000000000000000000000000000006000000000000000000000000000000000000000000000000000000000000000200000000000000000000000000000000000000000000000000000000000000007546f6b656e203100000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000100000000000000000000000000000000000000000000000000000000000000400000000000000000000000000000000000000000000000000000000000000060000000000000000000000000000000000000000000000000000000000000002000000000000000000000000000000000000000000000000000000000000000025431000000000000000000000000000000000000000000000000000000000000",
      );

    const result = await RaindexOrderBuilder.newWithDeployment(
      dotrainWithBuilder,
      undefined,
      "some-deployment",
    );
    const builder = extractWasmEncodedData(result);

    const builderConfig = extractWasmEncodedData<OrderBuilderCfg>(
      builder.getBuilderConfig(),
    );
    assert.equal(builderConfig.name, "Fixed limit");
    assert.equal(builderConfig.description, "Fixed limit order");
  });

  it("should initialize builder object with state update callback", async () => {
    const stateUpdateCallback = vi.fn();

    const result = await RaindexOrderBuilder.newWithDeployment(
      dotrainWithBuilder,
      undefined,
      "some-deployment",
      stateUpdateCallback,
    );
    const builder = extractWasmEncodedData(result);

    builder.executeStateUpdateCallback();
    assert.equal(stateUpdateCallback.mock.calls.length, 1);
  });

  it("should get order details", async () => {
    const result = RaindexOrderBuilder.getOrderDetails(
      dotrainWithBuilder,
      undefined,
    );
    const orderDetails = extractWasmEncodedData<NameAndDescriptionCfg>(result);
    assert.equal(orderDetails.name, "Fixed limit");
    assert.equal(orderDetails.description, "Fixed limit order");
    assert.equal(orderDetails.short_description, "Buy WETH with USDC on Base.");
  });

  it("should get deployment details", async () => {
    const result = RaindexOrderBuilder.getDeploymentDetails(
      dotrainWithBuilder,
      undefined,
    );
    const deploymentDetails =
      extractWasmEncodedData<Map<string, NameAndDescriptionCfg>>(result);
    const entries = Array.from(deploymentDetails.entries());
    assert.equal(entries[0][0], "other-deployment");
    assert.equal(entries[0][1].name, "Test test");
    assert.equal(entries[0][1].description, "Test test test");
    assert.equal(entries[1][0], "some-deployment");
    assert.equal(entries[1][1].name, "Buy WETH with USDC on Base.");
    assert.equal(
      entries[1][1].description,
      "Buy WETH with USDC for fixed price on Base network.",
    );
  });

  it("should get deployment detail", async () => {
    const result = RaindexOrderBuilder.getDeploymentDetail(
      dotrainWithBuilder,
      undefined,
      "other-deployment",
    );
    const deploymentDetail =
      extractWasmEncodedData<NameAndDescriptionCfg>(result);
    assert.equal(deploymentDetail.name, "Test test");
    assert.equal(deploymentDetail.description, "Test test test");
  });

  it("should get current deployment details", async () => {
    const result = await RaindexOrderBuilder.newWithDeployment(
      dotrainWithBuilder,
      undefined,
      "some-deployment",
    );
    const builder = extractWasmEncodedData(result);

    mockServer
      .forPost("/rpc-url")
      .withBodyIncluding("0x82ad56cb")
      .thenSendJsonRpcResult(
        "0x00000000000000000000000000000000000000000000000000000000000000200000000000000000000000000000000000000000000000000000000000000003000000000000000000000000000000000000000000000000000000000000006000000000000000000000000000000000000000000000000000000000000000e000000000000000000000000000000000000000000000000000000000000001a0000000000000000000000000000000000000000000000000000000000000000100000000000000000000000000000000000000000000000000000000000000400000000000000000000000000000000000000000000000000000000000000020000000000000000000000000000000000000000000000000000000000000000600000000000000000000000000000000000000000000000000000000000000010000000000000000000000000000000000000000000000000000000000000040000000000000000000000000000000000000000000000000000000000000006000000000000000000000000000000000000000000000000000000000000000200000000000000000000000000000000000000000000000000000000000000007546f6b656e203100000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000100000000000000000000000000000000000000000000000000000000000000400000000000000000000000000000000000000000000000000000000000000060000000000000000000000000000000000000000000000000000000000000002000000000000000000000000000000000000000000000000000000000000000025431000000000000000000000000000000000000000000000000000000000000",
      );

    const deploymentDetail = extractWasmEncodedData<NameAndDescriptionCfg>(
      builder.getCurrentDeploymentDetails(),
    );

    assert.equal(deploymentDetail.name, "Buy WETH with USDC on Base.");
    assert.equal(
      deploymentDetail.description,
      "Buy WETH with USDC for fixed price on Base network.",
    );
    assert.equal(
      deploymentDetail.short_description,
      "Buy WETH with USDC on Base.",
    );
  });

  it("should get token infos", async () => {
    mockServer
      .forPost("/rpc-url")
      .once()
      .withBodyIncluding("0x82ad56cb")
      .thenSendJsonRpcResult(
        "0x00000000000000000000000000000000000000000000000000000000000000200000000000000000000000000000000000000000000000000000000000000003000000000000000000000000000000000000000000000000000000000000006000000000000000000000000000000000000000000000000000000000000000e000000000000000000000000000000000000000000000000000000000000001a0000000000000000000000000000000000000000000000000000000000000000100000000000000000000000000000000000000000000000000000000000000400000000000000000000000000000000000000000000000000000000000000020000000000000000000000000000000000000000000000000000000000000000600000000000000000000000000000000000000000000000000000000000000010000000000000000000000000000000000000000000000000000000000000040000000000000000000000000000000000000000000000000000000000000006000000000000000000000000000000000000000000000000000000000000000200000000000000000000000000000000000000000000000000000000000000007546f6b656e203100000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000100000000000000000000000000000000000000000000000000000000000000400000000000000000000000000000000000000000000000000000000000000060000000000000000000000000000000000000000000000000000000000000002000000000000000000000000000000000000000000000000000000000000000025431000000000000000000000000000000000000000000000000000000000000",
      );
    mockServer
      .forPost("/rpc-url")
      .once()
      .withBodyIncluding("0x82ad56cb")
      .thenSendJsonRpcResult(
        "0x00000000000000000000000000000000000000000000000000000000000000200000000000000000000000000000000000000000000000000000000000000003000000000000000000000000000000000000000000000000000000000000006000000000000000000000000000000000000000000000000000000000000000e000000000000000000000000000000000000000000000000000000000000001a000000000000000000000000000000000000000000000000000000000000000010000000000000000000000000000000000000000000000000000000000000040000000000000000000000000000000000000000000000000000000000000002000000000000000000000000000000000000000000000000000000000000000060000000000000000000000000000000000000000000000000000000000000001000000000000000000000000000000000000000000000000000000000000004000000000000000000000000000000000000000000000000000000000000000600000000000000000000000000000000000000000000000000000000000000020000000000000000000000000000000000000000000000000000000000000000754656b656e203200000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000100000000000000000000000000000000000000000000000000000000000000400000000000000000000000000000000000000000000000000000000000000060000000000000000000000000000000000000000000000000000000000000002000000000000000000000000000000000000000000000000000000000000000025432000000000000000000000000000000000000000000000000000000000000",
      );
    const dotrainWithBuilder = `
    ${builderConfig2}

    ${dotrain}
    `;
    const result = await RaindexOrderBuilder.newWithDeployment(
      dotrainWithBuilder,
      undefined,
      "other-deployment",
    );
    const builder = extractWasmEncodedData(result);

    const token1TokenInfo = extractWasmEncodedData<TokenInfo>(
      await builder.getTokenInfo("token1"),
    );
    const token2TokenInfo = extractWasmEncodedData<TokenInfo>(
      await builder.getTokenInfo("token2"),
    );

    assert.equal(
      token1TokenInfo.address,
      "0xc2132d05d31c914a87c6611c10748aeb04b58e8f",
    );
    assert.equal(token1TokenInfo.decimals, 6);
    assert.equal(token1TokenInfo.name, "Token 1");
    assert.equal(token1TokenInfo.symbol, "T1");
    assert.equal(
      token2TokenInfo.address,
      "0x8f3cf7ad23cd3cadbd9735aff958023239c6a063",
    );
    assert.equal(token2TokenInfo.decimals, 18);
    assert.equal(token2TokenInfo.name, "Token 2");
    assert.equal(token2TokenInfo.symbol, "T2");
  });

  it("should get token infos", async () => {
    mockServer
      .forPost("/rpc-url")
      .once()
      .withBodyIncluding("0x82ad56cb")
      .thenSendJsonRpcResult(
        "0x00000000000000000000000000000000000000000000000000000000000000200000000000000000000000000000000000000000000000000000000000000003000000000000000000000000000000000000000000000000000000000000006000000000000000000000000000000000000000000000000000000000000000e000000000000000000000000000000000000000000000000000000000000001a0000000000000000000000000000000000000000000000000000000000000000100000000000000000000000000000000000000000000000000000000000000400000000000000000000000000000000000000000000000000000000000000020000000000000000000000000000000000000000000000000000000000000000600000000000000000000000000000000000000000000000000000000000000010000000000000000000000000000000000000000000000000000000000000040000000000000000000000000000000000000000000000000000000000000006000000000000000000000000000000000000000000000000000000000000000200000000000000000000000000000000000000000000000000000000000000007546f6b656e203100000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000100000000000000000000000000000000000000000000000000000000000000400000000000000000000000000000000000000000000000000000000000000060000000000000000000000000000000000000000000000000000000000000002000000000000000000000000000000000000000000000000000000000000000025431000000000000000000000000000000000000000000000000000000000000",
      );
    mockServer
      .forPost("/rpc-url")
      .once()
      .withBodyIncluding("0x82ad56cb")
      .thenSendJsonRpcResult(
        "0x00000000000000000000000000000000000000000000000000000000000000200000000000000000000000000000000000000000000000000000000000000003000000000000000000000000000000000000000000000000000000000000006000000000000000000000000000000000000000000000000000000000000000e000000000000000000000000000000000000000000000000000000000000001a000000000000000000000000000000000000000000000000000000000000000010000000000000000000000000000000000000000000000000000000000000040000000000000000000000000000000000000000000000000000000000000002000000000000000000000000000000000000000000000000000000000000000060000000000000000000000000000000000000000000000000000000000000001000000000000000000000000000000000000000000000000000000000000004000000000000000000000000000000000000000000000000000000000000000600000000000000000000000000000000000000000000000000000000000000020000000000000000000000000000000000000000000000000000000000000000754656b656e203200000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000100000000000000000000000000000000000000000000000000000000000000400000000000000000000000000000000000000000000000000000000000000060000000000000000000000000000000000000000000000000000000000000002000000000000000000000000000000000000000000000000000000000000000025432000000000000000000000000000000000000000000000000000000000000",
      );
    const dotrainWithBuilder = `
    ${builderConfig2}

    ${dotrain}
    `;
    const result = await RaindexOrderBuilder.newWithDeployment(
      dotrainWithBuilder,
      undefined,
      "other-deployment",
    );
    const builder = extractWasmEncodedData(result);

    const allTokenInfos = extractWasmEncodedData<TokenInfo[]>(
      await builder.getAllTokenInfos(),
    );

    assert.equal(allTokenInfos.length, 2);
    assert.equal(
      allTokenInfos[0].address,
      "0xc2132d05d31c914a87c6611c10748aeb04b58e8f",
    );
    assert.equal(allTokenInfos[0].decimals, 6);
    assert.equal(allTokenInfos[0].name, "Token 1");
    assert.equal(allTokenInfos[0].symbol, "T1");
    assert.equal(
      allTokenInfos[1].address,
      "0x8f3cf7ad23cd3cadbd9735aff958023239c6a063",
    );
    assert.equal(allTokenInfos[1].decimals, 18);
    assert.equal(allTokenInfos[1].name, "Token 2");
    assert.equal(allTokenInfos[1].symbol, "T2");
  });

  describe("deposit tests", async () => {
    let builder: RaindexOrderBuilder;
    let stateUpdateCallback: Mock;
    beforeEach(async () => {
      stateUpdateCallback = vi.fn();
      mockServer
        .forPost("/rpc-url")
        .withBodyIncluding("0x82ad56cb")
        .thenSendJsonRpcResult(
          "0x00000000000000000000000000000000000000000000000000000000000000200000000000000000000000000000000000000000000000000000000000000003000000000000000000000000000000000000000000000000000000000000006000000000000000000000000000000000000000000000000000000000000000e000000000000000000000000000000000000000000000000000000000000001a0000000000000000000000000000000000000000000000000000000000000000100000000000000000000000000000000000000000000000000000000000000400000000000000000000000000000000000000000000000000000000000000020000000000000000000000000000000000000000000000000000000000000000600000000000000000000000000000000000000000000000000000000000000010000000000000000000000000000000000000000000000000000000000000040000000000000000000000000000000000000000000000000000000000000006000000000000000000000000000000000000000000000000000000000000000200000000000000000000000000000000000000000000000000000000000000007546f6b656e203100000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000100000000000000000000000000000000000000000000000000000000000000400000000000000000000000000000000000000000000000000000000000000060000000000000000000000000000000000000000000000000000000000000002000000000000000000000000000000000000000000000000000000000000000025431000000000000000000000000000000000000000000000000000000000000",
        );
      const result = await RaindexOrderBuilder.newWithDeployment(
        dotrainWithBuilder,
        undefined,
        "some-deployment",
        stateUpdateCallback,
      );
      builder = extractWasmEncodedData(result);
    });

    it("should add deposit", async () => {
      assert.equal(
        extractWasmEncodedData<boolean>(builder.hasAnyDeposit()),
        false,
      );

      await builder.setDeposit("token1", "50.6");
      const deposits = extractWasmEncodedData<TokenDeposit[]>(
        builder.getDeposits(),
      );
      assert.equal(deposits.length, 1);

      assert.equal(
        extractWasmEncodedData<boolean>(builder.hasAnyDeposit()),
        true,
      );

      assert.equal(stateUpdateCallback.mock.calls.length, 1);
      expect(stateUpdateCallback).toHaveBeenCalledWith(
        extractWasmEncodedData(builder.serializeState()),
      );
    });

    it("should update deposit", async () => {
      await builder.setDeposit("token1", "50.6");
      await builder.setDeposit("token1", "100.6");
      const deposits = extractWasmEncodedData<TokenDeposit[]>(
        builder.getDeposits(),
      );
      assert.equal(deposits.length, 1);
      assert.equal(deposits[0].amount, "100.6");

      assert.equal(stateUpdateCallback.mock.calls.length, 2);
      expect(stateUpdateCallback).toHaveBeenCalledWith(
        extractWasmEncodedData(builder.serializeState()),
      );
    });

    it("should throw error if deposit token is not found in builder config", () => {
      const result = builder.getDepositPresets("token3");
      if (!result.error) expect.fail("Expected error");
      expect(result.error.msg).toBe(
        "Deposit token not found in builder config: token3",
      );
      expect(result.error.readableMsg).toBe(
        "The deposit token 'token3' was not found in the YAML configuration.",
      );
    });

    it("should remove deposit", async () => {
      await builder.setDeposit("token1", "50.6");
      const deposits = extractWasmEncodedData<TokenDeposit[]>(
        builder.getDeposits(),
      );
      assert.equal(deposits.length, 1);

      builder.unsetDeposit("token1");
      const depositsAfterRemove = extractWasmEncodedData<TokenDeposit[]>(
        builder.getDeposits(),
      );
      assert.equal(depositsAfterRemove.length, 0);

      await builder.setDeposit("token1", "50.6");
      assert.equal(
        extractWasmEncodedData<TokenDeposit[]>(builder.getDeposits()).length,
        1,
      );

      assert.equal(stateUpdateCallback.mock.calls.length, 3);
      expect(stateUpdateCallback).toHaveBeenCalledWith(
        extractWasmEncodedData(builder.serializeState()),
      );
    });

    it("should throw error if deposit amount is empty", async () => {
      const result = await builder.setDeposit("token1", "");
      if (!result.error) expect.fail("Expected error");
      expect(result.error.msg).toBe("Deposit amount cannot be an empty string");
      expect(result.error.readableMsg).toBe(
        "The deposit amount cannot be an empty string. Please set a valid amount.",
      );
    });

    it("should get deposit presets", async () => {
      const presets = extractWasmEncodedData<string[]>(
        builder.getDepositPresets("token1"),
      );
      assert.equal(presets.length, 5);
      assert.equal(presets[0], "0");
      assert.equal(presets[1], "10");
      assert.equal(presets[2], "100");
      assert.equal(presets[3], "1000");
      assert.equal(presets[4], "10000");
    });

    it("should throw error if deposit token is not found in builder config", () => {
      const result = builder.getDepositPresets("token2");
      if (!result.error) expect.fail("Expected error");
      expect(result.error.msg).toBe(
        "Deposit token not found in builder config: token2",
      );
      expect(result.error.readableMsg).toBe(
        "The deposit token 'token2' was not found in the YAML configuration.",
      );
    });
  });

  describe("field value tests", async () => {
    let builder: RaindexOrderBuilder;
    let stateUpdateCallback: Mock;
    beforeEach(async () => {
      stateUpdateCallback = vi.fn();
      mockServer
        .forPost("/rpc-url")
        .withBodyIncluding("0x82ad56cb")
        .thenSendJsonRpcResult(
          "0x00000000000000000000000000000000000000000000000000000000000000200000000000000000000000000000000000000000000000000000000000000003000000000000000000000000000000000000000000000000000000000000006000000000000000000000000000000000000000000000000000000000000000e000000000000000000000000000000000000000000000000000000000000001a0000000000000000000000000000000000000000000000000000000000000000100000000000000000000000000000000000000000000000000000000000000400000000000000000000000000000000000000000000000000000000000000020000000000000000000000000000000000000000000000000000000000000000600000000000000000000000000000000000000000000000000000000000000010000000000000000000000000000000000000000000000000000000000000040000000000000000000000000000000000000000000000000000000000000006000000000000000000000000000000000000000000000000000000000000000200000000000000000000000000000000000000000000000000000000000000007546f6b656e203100000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000100000000000000000000000000000000000000000000000000000000000000400000000000000000000000000000000000000000000000000000000000000060000000000000000000000000000000000000000000000000000000000000002000000000000000000000000000000000000000000000000000000000000000025431000000000000000000000000000000000000000000000000000000000000",
        );
      const result = await RaindexOrderBuilder.newWithDeployment(
        dotrainWithBuilder,
        undefined,
        "some-deployment",
        stateUpdateCallback,
      );
      builder = extractWasmEncodedData(result);
    });

    it("should save the field value as presets", async () => {
      const allFieldDefinitions = extractWasmEncodedData<
        OrderBuilderFieldDefinitionCfg[]
      >(builder.getAllFieldDefinitions());
      builder.setFieldValue(
        "binding-1",
        allFieldDefinitions[0].presets?.[0]?.value || "",
      );
      assert.deepEqual(
        extractWasmEncodedData<FieldValue>(builder.getFieldValue("binding-1")),
        {
          field: "binding-1",
          value: allFieldDefinitions[0].presets?.[0]?.value || "",
          is_preset: true,
        },
      );
      builder.setFieldValue(
        "binding-1",
        allFieldDefinitions[0].presets?.[1]?.value || "",
      );
      assert.deepEqual(
        extractWasmEncodedData<FieldValue>(builder.getFieldValue("binding-1")),
        {
          field: "binding-1",
          value: allFieldDefinitions[0].presets?.[1]?.value || "",
          is_preset: true,
        },
      );
      builder.setFieldValue(
        "binding-1",
        allFieldDefinitions[0].presets?.[2]?.value || "",
      );
      assert.deepEqual(
        extractWasmEncodedData<FieldValue>(builder.getFieldValue("binding-1")),
        {
          field: "binding-1",
          value: allFieldDefinitions[0].presets?.[2]?.value || "",
          is_preset: true,
        },
      );

      assert.equal(stateUpdateCallback.mock.calls.length, 3);
      expect(stateUpdateCallback).toHaveBeenCalledWith(
        extractWasmEncodedData(builder.serializeState()),
      );
    });

    it("should save field value as custom values", async () => {
      builder.setFieldValues([
        {
          field: "binding-1",
          value: "0x1234567890abcdef1234567890abcdef12345678",
        },
        {
          field: "binding-2",
          value: "100",
        },
      ]);
      builder.setFieldValues([
        {
          field: "binding-1",
          value: "some-string",
        },
        {
          field: "binding-2",
          value: "true",
        },
      ]);
      const fieldValues = extractWasmEncodedData<FieldValue[]>(
        builder.getAllFieldValues(),
      );
      assert.equal(fieldValues.length, 2);
      assert.deepEqual(fieldValues[0], {
        field: "binding-1",
        value: "some-string",
        is_preset: true,
      });
      assert.deepEqual(fieldValues[1], {
        field: "binding-2",
        value: "true",
        is_preset: false,
      });

      assert.equal(stateUpdateCallback.mock.calls.length, 4);
      expect(stateUpdateCallback).toHaveBeenCalledWith(
        extractWasmEncodedData(builder.serializeState()),
      );
    });

    it("should throw error during save if field binding is not found in field definitions", () => {
      const result = builder.setFieldValue("binding-3", "1");
      if (!result.error) expect.fail("Expected error");
      expect(result.error.msg).toBe("Field binding not found: binding-3");
      expect(result.error.readableMsg).toBe(
        "The field binding 'binding-3' could not be found in the YAML configuration.",
      );
    });

    it("should get field value", async () => {
      builder.setFieldValue(
        "binding-1",
        "0x1234567890abcdef1234567890abcdef12345678",
      );
      let fieldValue = extractWasmEncodedData<FieldValue>(
        builder.getFieldValue("binding-1"),
      );
      assert.deepEqual(fieldValue, {
        field: "binding-1",
        value: "0x1234567890abcdef1234567890abcdef12345678",
        is_preset: true,
      });

      builder.setFieldValue("binding-2", "true");
      fieldValue = extractWasmEncodedData<FieldValue>(
        builder.getFieldValue("binding-2"),
      );
      assert.deepEqual(fieldValue, {
        field: "binding-2",
        value: "true",
        is_preset: false,
      });

      builder.setFieldValue("binding-1", "some-string");
      fieldValue = extractWasmEncodedData<FieldValue>(
        builder.getFieldValue("binding-1"),
      );
      assert.deepEqual(fieldValue, {
        field: "binding-1",
        value: "some-string",
        is_preset: true,
      });

      builder.setFieldValue("binding-2", "100.5");
      fieldValue = extractWasmEncodedData<FieldValue>(
        builder.getFieldValue("binding-2"),
      );
      assert.deepEqual(fieldValue, {
        field: "binding-2",
        value: "100.5",
        is_preset: false,
      });
    });

    it("should throw error during get if field binding is not found", () => {
      const result = builder.getFieldValue("binding-3");
      if (!result.error) expect.fail("Expected error");
      expect(result.error.msg).toBe("Field binding not found: binding-3");
      expect(result.error.readableMsg).toBe(
        "The field binding 'binding-3' could not be found in the YAML configuration.",
      );
    });

    it("should correctly filter field definitions", async () => {
      const allFieldDefinitions = extractWasmEncodedData<
        OrderBuilderFieldDefinitionCfg[]
      >(builder.getAllFieldDefinitions());
      assert.equal(allFieldDefinitions.length, 2);

      const fieldDefinitionsWithoutDefaults = extractWasmEncodedData<
        OrderBuilderFieldDefinitionCfg[]
      >(builder.getAllFieldDefinitions(true));
      assert.equal(fieldDefinitionsWithoutDefaults.length, 1);
      assert.equal(fieldDefinitionsWithoutDefaults[0].binding, "binding-1");

      const fieldDefinitionsWithDefaults = extractWasmEncodedData<
        OrderBuilderFieldDefinitionCfg[]
      >(builder.getAllFieldDefinitions(false));
      assert.equal(fieldDefinitionsWithDefaults.length, 1);
      assert.equal(fieldDefinitionsWithDefaults[0].binding, "binding-2");
    });
  });

  describe("field definition tests", async () => {
    let builder: RaindexOrderBuilder;
    beforeAll(async () => {
      mockServer
        .forPost("/rpc-url")
        .withBodyIncluding("0x82ad56cb")
        .thenSendJsonRpcResult(
          "0x00000000000000000000000000000000000000000000000000000000000000200000000000000000000000000000000000000000000000000000000000000003000000000000000000000000000000000000000000000000000000000000006000000000000000000000000000000000000000000000000000000000000000e000000000000000000000000000000000000000000000000000000000000001a0000000000000000000000000000000000000000000000000000000000000000100000000000000000000000000000000000000000000000000000000000000400000000000000000000000000000000000000000000000000000000000000020000000000000000000000000000000000000000000000000000000000000000600000000000000000000000000000000000000000000000000000000000000010000000000000000000000000000000000000000000000000000000000000040000000000000000000000000000000000000000000000000000000000000006000000000000000000000000000000000000000000000000000000000000000200000000000000000000000000000000000000000000000000000000000000007546f6b656e203100000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000100000000000000000000000000000000000000000000000000000000000000400000000000000000000000000000000000000000000000000000000000000060000000000000000000000000000000000000000000000000000000000000002000000000000000000000000000000000000000000000000000000000000000025431000000000000000000000000000000000000000000000000000000000000",
        );
      const result = await RaindexOrderBuilder.newWithDeployment(
        dotrainWithBuilder,
        undefined,
        "some-deployment",
      );
      builder = extractWasmEncodedData(result);
    });

    it("should get field definition", async () => {
      const allFieldDefinitions = extractWasmEncodedData<
        OrderBuilderFieldDefinitionCfg[]
      >(builder.getAllFieldDefinitions());
      assert.equal(allFieldDefinitions.length, 2);

      const fieldDefinition =
        extractWasmEncodedData<OrderBuilderFieldDefinitionCfg>(
          builder.getFieldDefinition("binding-1"),
        );
      assert.equal(fieldDefinition.name, "Field 1 name");
      assert.equal(fieldDefinition.description, "Field 1 description");
      assert.equal(fieldDefinition.presets?.length, 3);
      assert.equal(fieldDefinition.default, "some-default-value");
      assert.equal(fieldDefinition.showCustomField, undefined);

      let presets = fieldDefinition.presets as OrderBuilderPresetCfg[];
      assert.equal(presets[0].name, "Preset 1");
      assert.equal(
        presets[0].value,
        "0x1234567890abcdef1234567890abcdef12345678",
      );
      assert.equal(presets[1].name, "Preset 2");
      assert.equal(presets[1].value, "false");
      assert.equal(presets[2].name, "Preset 3");
      assert.equal(presets[2].value, "some-string");

      const fieldDefinition2 =
        extractWasmEncodedData<OrderBuilderFieldDefinitionCfg>(
          builder.getFieldDefinition("binding-2"),
        );
      presets = fieldDefinition2.presets as OrderBuilderPresetCfg[];
      assert.equal(presets[0].value, "99.2");
      assert.equal(presets[1].value, "582.1");
      assert.equal(presets[2].value, "648.239");
      assert.equal(fieldDefinition2.default, undefined);
      assert.equal(fieldDefinition2.showCustomField, true);
    });

    it("should throw error during get if field binding is not found", () => {
      const result = builder.getFieldDefinition("binding-3");
      if (!result.error) expect.fail("Expected error");
      expect(result.error.msg).toBe("Field binding not found: binding-3");
      expect(result.error.readableMsg).toBe(
        "The field binding 'binding-3' could not be found in the YAML configuration.",
      );
    });
  });

  describe("state management tests", async () => {
    let serializedState =
      "H4sIAAAAAAAA_21QsU7DMBCNAwIJMSDEioTEionrKEmpylBQVYZKIJFKdAyp20R17GC7BMRHMLLyAxVfwMrG9yA2iLBDo_YGv_O9d757BtZfbGtURCp4m7JRyiZA15C1tcjeR3RGbF3ZMAyfEtawTKxr9NCxX5PgSrKmsYEQWPUYrt_MgpJnBDKiCi6mpm9fY6JU3nIcyuOIJlyqVhM1PUfkMZwJ-lQqQHkCM7obXuzp9Ln9NT_8bM_fX7y37xsbn3y8xmAXbGo6LHc4wMDYDn992NZ_1L-hGuD7PljyVbGu6x7p9KzI6aB7FWZQBQMpewGO-kHnYdznBU2HdxMu8HnvujO-FMPTHd3DVUIEHJGc8seMMPUDQuHlHcsBAAA=";
    let dotrain3: string;
    let builder: RaindexOrderBuilder;
    beforeAll(async () => {
      mockServer
        .forPost("/rpc-url")
        .once()
        .withBodyIncluding("0x82ad56cb")
        .thenSendJsonRpcResult(
          "0x00000000000000000000000000000000000000000000000000000000000000200000000000000000000000000000000000000000000000000000000000000003000000000000000000000000000000000000000000000000000000000000006000000000000000000000000000000000000000000000000000000000000000e000000000000000000000000000000000000000000000000000000000000001a0000000000000000000000000000000000000000000000000000000000000000100000000000000000000000000000000000000000000000000000000000000400000000000000000000000000000000000000000000000000000000000000020000000000000000000000000000000000000000000000000000000000000000600000000000000000000000000000000000000000000000000000000000000010000000000000000000000000000000000000000000000000000000000000040000000000000000000000000000000000000000000000000000000000000006000000000000000000000000000000000000000000000000000000000000000200000000000000000000000000000000000000000000000000000000000000007546f6b656e203100000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000100000000000000000000000000000000000000000000000000000000000000400000000000000000000000000000000000000000000000000000000000000060000000000000000000000000000000000000000000000000000000000000002000000000000000000000000000000000000000000000000000000000000000025431000000000000000000000000000000000000000000000000000000000000",
        );
      mockServer
        .forPost("/rpc-url")
        .once()
        .withBodyIncluding("0x82ad56cb")
        .thenSendJsonRpcResult(
          "0x00000000000000000000000000000000000000000000000000000000000000200000000000000000000000000000000000000000000000000000000000000003000000000000000000000000000000000000000000000000000000000000006000000000000000000000000000000000000000000000000000000000000000e000000000000000000000000000000000000000000000000000000000000001a000000000000000000000000000000000000000000000000000000000000000010000000000000000000000000000000000000000000000000000000000000040000000000000000000000000000000000000000000000000000000000000002000000000000000000000000000000000000000000000000000000000000000060000000000000000000000000000000000000000000000000000000000000001000000000000000000000000000000000000000000000000000000000000004000000000000000000000000000000000000000000000000000000000000000600000000000000000000000000000000000000000000000000000000000000020000000000000000000000000000000000000000000000000000000000000000754656b656e203200000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000100000000000000000000000000000000000000000000000000000000000000400000000000000000000000000000000000000000000000000000000000000060000000000000000000000000000000000000000000000000000000000000002000000000000000000000000000000000000000000000000000000000000000025432000000000000000000000000000000000000000000000000000000000000",
        );
      dotrain3 = `${builderConfig3}

${dotrain}`;
      const result = await RaindexOrderBuilder.newWithDeployment(
        dotrain3,
        undefined,
        "other-deployment",
      );
      builder = extractWasmEncodedData(result);

      builder.setFieldValue(
        "test-binding",
        extractWasmEncodedData<OrderBuilderFieldDefinitionCfg>(
          builder.getFieldDefinition("test-binding"),
        ).presets?.[0].value || "",
      );
      await builder.setDeposit("token1", "50.6");
      await builder.setDeposit("token2", "100");
      builder.unsetSelectToken("token1");
      await builder.setSelectToken(
        "token1",
        "0x6666666666666666666666666666666666666666",
      );
      builder.setVaultId("input", "token1", "666");
      builder.setVaultId("output", "token2", "333");
    });

    it("should serialize builder state", async () => {
      const serialized = extractWasmEncodedData<string>(
        builder.serializeState(),
      );
      assert.equal(serialized, serializedState);
    });

    it("should deserialize builder state", async () => {
      const builderResult = await RaindexOrderBuilder.newFromState(
        dotrain3,
        undefined,
        serializedState,
      );
      const builder = extractWasmEncodedData(builderResult);

      const fieldValues = extractWasmEncodedData<FieldValue[]>(
        builder.getAllFieldValues(),
      );
      assert.equal(fieldValues.length, 1);
      assert.deepEqual(fieldValues[0], {
        field: "test-binding",
        value: "test-value",
        is_preset: true,
      });

      assert.equal(
        extractWasmEncodedData<boolean>(builder.isSelectTokenSet("token1")),
        true,
      );
      assert.equal(
        extractWasmEncodedData<boolean>(builder.isSelectTokenSet("token2")),
        true,
      );
      const deposits = extractWasmEncodedData<TokenDeposit[]>(
        builder.getDeposits(),
      );
      assert.equal(deposits.length, 2);
      assert.equal(deposits[0].token, "token1");
      assert.equal(deposits[0].amount, "50.6");
      assert.equal(
        deposits[0].address,
        "0xc2132d05d31c914a87c6611c10748aeb04b58e8f",
      );
      assert.equal(deposits[1].token, "token2");
      assert.equal(deposits[1].amount, "100");
      assert.equal(
        deposits[1].address,
        "0x8f3cf7ad23cd3cadbd9735aff958023239c6a063",
      );

      const result = builder.getCurrentDeployment();
      const builderDeployment =
        extractWasmEncodedData<OrderBuilderDeploymentCfg>(result);
      assert.equal(
        builderDeployment.deployment.order.inputs[0].vaultId,
        "0x29a",
      );
      assert.equal(
        builderDeployment.deployment.order.outputs[0].vaultId,
        "0x14d",
      );
    });

    it("should throw error if given dotrain is different", async () => {
      let testDotrain = `${builderConfig}

${dotrainWithTokensMismatch}`;
      const result = await RaindexOrderBuilder.newFromState(
        testDotrain,
        undefined,
        serializedState,
      );
      if (!result.error) expect.fail("Expected error");
      expect(result.error.msg).toBe("Deserialized dotrain mismatch");
      expect(result.error.readableMsg).toBe(
        "There was a mismatch in the dotrain configuration. Please check your YAML configuration for consistency.",
      );
    });

    it("should keep the same vault ids after deserializing if not set during serializing", async () => {
      mockServer
        .forPost("/rpc-url")
        .withBodyIncluding("0x82ad56cb")
        .thenSendJsonRpcResult(
          "0x00000000000000000000000000000000000000000000000000000000000000200000000000000000000000000000000000000000000000000000000000000003000000000000000000000000000000000000000000000000000000000000006000000000000000000000000000000000000000000000000000000000000000e000000000000000000000000000000000000000000000000000000000000001a0000000000000000000000000000000000000000000000000000000000000000100000000000000000000000000000000000000000000000000000000000000400000000000000000000000000000000000000000000000000000000000000020000000000000000000000000000000000000000000000000000000000000000600000000000000000000000000000000000000000000000000000000000000010000000000000000000000000000000000000000000000000000000000000040000000000000000000000000000000000000000000000000000000000000006000000000000000000000000000000000000000000000000000000000000000200000000000000000000000000000000000000000000000000000000000000007546f6b656e203100000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000100000000000000000000000000000000000000000000000000000000000000400000000000000000000000000000000000000000000000000000000000000060000000000000000000000000000000000000000000000000000000000000002000000000000000000000000000000000000000000000000000000000000000025431000000000000000000000000000000000000000000000000000000000000",
        );

      let testDotrain = `
${builderConfig2}

${dotrainWithoutVaultIds}
	  `;
      let result = await RaindexOrderBuilder.newWithDeployment(
        testDotrain,
        undefined,
        "other-deployment",
      );
      builder = extractWasmEncodedData(result);

      let deployment1 = extractWasmEncodedData<OrderBuilderDeploymentCfg>(
        builder.getCurrentDeployment(),
      );
      assert.equal(deployment1.deployment.order.inputs[0].vaultId, undefined);
      assert.equal(deployment1.deployment.order.outputs[0].vaultId, undefined);

      let serializedState = extractWasmEncodedData<string>(
        builder.serializeState(),
      );
      const builderResult = await RaindexOrderBuilder.newFromState(
        testDotrain,
        undefined,
        serializedState,
      );
      builder = extractWasmEncodedData(builderResult);

      let deployment2 = extractWasmEncodedData<OrderBuilderDeploymentCfg>(
        builder.getCurrentDeployment(),
      );
      assert.equal(deployment2.deployment.order.inputs[0].vaultId, undefined);
      assert.equal(deployment2.deployment.order.outputs[0].vaultId, undefined);
    });

    it("should get all the yaml fields", async () => {
      mockServer
        .forPost("/rpc-url")
        .once()
        .thenSendJsonRpcResult(
          "0x00000000000000000000000000000000000000000000000000000000015db63900000000000000000000000000000000000000000000000000000000000000400000000000000000000000000000000000000000000000000000000000000003000000000000000000000000000000000000000000000000000000000000006000000000000000000000000000000000000000000000000000000000000000a000000000000000000000000000000000000000000000000000000000000001200000000000000000000000000000000000000000000000000000000000000020000000000000000000000000000000000000000000000000000000000000000600000000000000000000000000000000000000000000000000000000000000600000000000000000000000000000000000000000000000000000000000000020000000000000000000000000000000000000000000000000000000000000000a54657468657220555344000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000060000000000000000000000000000000000000000000000000000000000000002000000000000000000000000000000000000000000000000000000000000000045553445400000000000000000000000000000000000000000000000000000000",
        );
      mockServer
        .forPost("/rpc-url")
        .once()
        .thenSendJsonRpcResult(
          "0x00000000000000000000000000000000000000000000000000000000015db63900000000000000000000000000000000000000000000000000000000000000400000000000000000000000000000000000000000000000000000000000000003000000000000000000000000000000000000000000000000000000000000006000000000000000000000000000000000000000000000000000000000000000a000000000000000000000000000000000000000000000000000000000000001200000000000000000000000000000000000000000000000000000000000000020000000000000000000000000000000000000000000000000000000000000000600000000000000000000000000000000000000000000000000000000000000600000000000000000000000000000000000000000000000000000000000000020000000000000000000000000000000000000000000000000000000000000000a54657468657220555344000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000060000000000000000000000000000000000000000000000000000000000000002000000000000000000000000000000000000000000000000000000000000000045553445400000000000000000000000000000000000000000000000000000000",
        );
      dotrain3 = `${builderConfig}

${dotrain}`;
      const result = await RaindexOrderBuilder.newWithDeployment(
        dotrain3,
        undefined,
        "some-deployment",
      );
      const builder = extractWasmEncodedData(result);

      const {
        fieldDefinitionsWithoutDefaults,
        fieldDefinitionsWithDefaults,
        deposits,
        orderInputs,
        orderOutputs,
      } = extractWasmEncodedData<AllBuilderConfig>(
        builder.getAllBuilderConfig(),
      );

      assert.equal(fieldDefinitionsWithoutDefaults.length, 1);
      assert.equal(fieldDefinitionsWithoutDefaults[0].binding, "binding-2");

      assert.equal(fieldDefinitionsWithDefaults.length, 1);
      assert.equal(fieldDefinitionsWithDefaults[0].binding, "binding-1");

      assert.equal(deposits.length, 1);
      assert.equal(deposits[0].token?.key, "token1");

      assert.equal(orderInputs.length, 1);
      assert.equal(orderInputs[0].token?.key, "token1");

      assert.equal(orderOutputs.length, 1);
      assert.equal(orderOutputs[0].token?.key, "token2");
    });
  });

  describe("order operations tests", async () => {
    let builder: RaindexOrderBuilder;

    beforeEach(async () => {
      // token1 info
      mockServer
        .forPost("/rpc-url")
        .once()
        .withBodyIncluding("0x82ad56cb")
        .thenSendJsonRpcResult(
          "0x00000000000000000000000000000000000000000000000000000000000000200000000000000000000000000000000000000000000000000000000000000003000000000000000000000000000000000000000000000000000000000000006000000000000000000000000000000000000000000000000000000000000000e000000000000000000000000000000000000000000000000000000000000001a0000000000000000000000000000000000000000000000000000000000000000100000000000000000000000000000000000000000000000000000000000000400000000000000000000000000000000000000000000000000000000000000020000000000000000000000000000000000000000000000000000000000000000600000000000000000000000000000000000000000000000000000000000000010000000000000000000000000000000000000000000000000000000000000040000000000000000000000000000000000000000000000000000000000000006000000000000000000000000000000000000000000000000000000000000000200000000000000000000000000000000000000000000000000000000000000007546f6b656e203100000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000100000000000000000000000000000000000000000000000000000000000000400000000000000000000000000000000000000000000000000000000000000060000000000000000000000000000000000000000000000000000000000000002000000000000000000000000000000000000000000000000000000000000000025431000000000000000000000000000000000000000000000000000000000000",
        );
      // token2 info
      mockServer
        .forPost("/rpc-url")
        .once()
        .withBodyIncluding("0x82ad56cb")
        .thenSendJsonRpcResult(
          "0x00000000000000000000000000000000000000000000000000000000000000200000000000000000000000000000000000000000000000000000000000000003000000000000000000000000000000000000000000000000000000000000006000000000000000000000000000000000000000000000000000000000000000e000000000000000000000000000000000000000000000000000000000000001a000000000000000000000000000000000000000000000000000000000000000010000000000000000000000000000000000000000000000000000000000000040000000000000000000000000000000000000000000000000000000000000002000000000000000000000000000000000000000000000000000000000000000120000000000000000000000000000000000000000000000000000000000000001000000000000000000000000000000000000000000000000000000000000004000000000000000000000000000000000000000000000000000000000000000600000000000000000000000000000000000000000000000000000000000000020000000000000000000000000000000000000000000000000000000000000000754656b656e203200000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000100000000000000000000000000000000000000000000000000000000000000400000000000000000000000000000000000000000000000000000000000000060000000000000000000000000000000000000000000000000000000000000002000000000000000000000000000000000000000000000000000000000000000025432000000000000000000000000000000000000000000000000000000000000",
        );

      let dotrain2 = `
      ${builderConfig2}

      ${dotrain}
      `;
      const result = await RaindexOrderBuilder.newWithDeployment(
        dotrain2,
        undefined,
        "other-deployment",
      );
      builder = extractWasmEncodedData(result);
    });

    it("checks input and output allowances", async () => {
      // token2 allowance
      await mockServer
        .forPost("/rpc-url")
        .withBodyIncluding("0x8f3cf7ad23cd3cadbd9735aff958023239c6a063")
        .thenSendJsonRpcResult(
          "0x0000000000000000000000000000000000000000000000000000000000000001",
        );

      await builder.setDeposit("token2", "200");

      const allowances = extractWasmEncodedData<TokenAllowance[]>(
        await builder.checkAllowances(
          "0x1234567890abcdef1234567890abcdef12345678",
        ),
      );
      assert.equal(allowances.length, 1);
      assert.equal(
        allowances[0].token,
        "0x8f3cf7ad23cd3cadbd9735aff958023239c6a063",
      );
      assert.equal(allowances[0].allowance, "0x1");
    });

    it("generates approval calldatas", async () => {
      // decimal call
      await mockServer
        .forPost("/rpc-url")
        .once()
        .thenSendJsonRpcResult(
          "0x0000000000000000000000000000000000000000000000000000000000000012",
        );
      // allowance - 1000 * 10^18
      await mockServer
        .forPost("/rpc-url")
        .once()
        .thenSendJsonRpcResult(
          "0x00000000000000000000000000000000000000000000003635C9ADC5DEA00000",
        );
      // decimal call
      await mockServer
        .forPost("/rpc-url")
        .once()
        .thenSendJsonRpcResult(
          "0x0000000000000000000000000000000000000000000000000000000000000012",
        );
      // allowance - 1000 * 10^18
      await mockServer
        .forPost("/rpc-url")
        .once()
        .thenSendJsonRpcResult(
          "0x00000000000000000000000000000000000000000000003635C9ADC5DEA00000",
        );

      await builder.setDeposit("token1", "1000");
      await builder.setDeposit("token2", "5000");

      const result = extractWasmEncodedData<ApprovalCalldataResult>(
        await builder.generateApprovalCalldatas(
          "0x1234567890abcdef1234567890abcdef12345678",
        ),
      );

      // @ts-expect-error - result is valid
      assert.equal(result.Calldatas.length, 1);
      // @ts-expect-error - result is valid
      assert.equal(
        result.Calldatas[0].token,
        "0x8f3cf7ad23cd3cadbd9735aff958023239c6a063",
      );
      assert.equal(
        // @ts-expect-error - result is valid
        result.Calldatas[0].calldata,
        "0x095ea7b3000000000000000000000000c95a5f8efe14d7a20bd2e5bafec4e71f8ce0b9a600000000000000000000000000000000000000000000010f0cf064dd59200000",
      );

      // Test no deposits case
      builder.unsetDeposit("token1");
      builder.unsetDeposit("token2");
      const emptyResult = extractWasmEncodedData<ApprovalCalldataResult>(
        await builder.generateApprovalCalldatas(
          "0x1234567890abcdef1234567890abcdef12345678",
        ),
      );
      assert.equal(emptyResult, "NoDeposits");
    });

    it("overwrites approvals when allowance is higher than deposit", async () => {
      // decimal call
      await mockServer
        .forPost("/rpc-url")
        .once()
        .thenSendJsonRpcResult(
          "0x0000000000000000000000000000000000000000000000000000000000000012",
        );
      // allowance - 5000 * 10^18
      await mockServer
        .forPost("/rpc-url")
        .once()
        .thenSendJsonRpcResult(
          "0x00000000000000000000000000000000000000000000010f0cf064dd59200000",
        );

      builder.unsetDeposit("token1");
      await builder.setDeposit("token2", "2000");

      const result = extractWasmEncodedData<ApprovalCalldataResult>(
        await builder.generateApprovalCalldatas(
          "0x1234567890abcdef1234567890abcdef12345678",
        ),
      );

      // @ts-expect-error - result is valid
      assert.equal(result.Calldatas.length, 1);
      // @ts-expect-error - result is valid
      assert.equal(
        result.Calldatas[0].token,
        "0x8f3cf7ad23cd3cadbd9735aff958023239c6a063",
      );
      assert.equal(
        // @ts-expect-error - result is valid
        result.Calldatas[0].calldata,
        "0x095ea7b3000000000000000000000000c95a5f8efe14d7a20bd2e5bafec4e71f8ce0b9a600000000000000000000000000000000000000000000006c6b935b8bbd400000",
      );
    });

    it("generates deposit calldatas", async () => {
      await mockServer
        .forPost("/rpc-url")
        .withBodyIncluding("0x56fb83e9")
        .thenSendJsonRpcResult(`0x${"0".repeat(24) + "1".repeat(40)}`);
      // I_STORE()() call
      await mockServer
        .forPost("/rpc-url")
        .withBodyIncluding("0x251ac32e")
        .thenSendJsonRpcResult(`0x${"0".repeat(24) + "2".repeat(40)}`);
      // I_PARSER() call
      await mockServer
        .forPost("/rpc-url")
        .withBodyIncluding("0xf79693f4")
        .thenSendJsonRpcResult(`0x${"0".repeat(24) + "3".repeat(40)}`);
      // parse2() call
      await mockServer
        .forPost("/rpc-url")
        .withBodyIncluding("0xa3869e14")
        // 0x1234 encoded bytes
        .thenSendJsonRpcResult(
          "0x000000000000000000000000000000000000000000000000000000000000002000000000000000000000000000000000000000000000000000000000000000021234000000000000000000000000000000000000000000000000000000000000",
        );

      await builder.setDeposit("token1", "1000");
      await builder.setDeposit("token2", "5000");

      const result = extractWasmEncodedData<DepositCalldataResult>(
        await builder.generateDepositCalldatas(),
      );

      // @ts-expect-error - result is valid
      assert.equal(result.Calldatas.length, 1);
      assert.equal(
        // @ts-expect-error - result is valid
        result.Calldatas[0],
        "0xca9109ba0000000000000000000000008f3cf7ad23cd3cadbd9735aff958023239c6a0630000000000000000000000000000000000000000000000000000000000000001000000000000000000000000000000000000000000000000000000000000138800000000000000000000000000000000000000000000000000000000000000800000000000000000000000000000000000000000000000000000000000000000",
      );

      // Test no deposits case
      builder.unsetDeposit("token1");
      builder.unsetDeposit("token2");
      const emptyResult = extractWasmEncodedData<DepositCalldataResult>(
        await builder.generateDepositCalldatas(),
      );
      assert.equal(emptyResult, "NoDeposits");
    });

    it("generates add order calldata", async () => {
      await mockRainlangRegistryCalls();
      await mockServer
        .forPost("/rpc-url")
        .withBodyIncluding("0x56fb83e9")
        .thenSendJsonRpcResult(`0x${"0".repeat(24) + "1".repeat(40)}`);
      // I_STORE()() call
      await mockServer
        .forPost("/rpc-url")
        .withBodyIncluding("0x251ac32e")
        .thenSendJsonRpcResult(`0x${"0".repeat(24) + "2".repeat(40)}`);
      // I_PARSER() call
      await mockServer
        .forPost("/rpc-url")
        .withBodyIncluding("0xf79693f4")
        .thenSendJsonRpcResult(`0x${"0".repeat(24) + "3".repeat(40)}`);
      // parse2() call
      await mockServer
        .forPost("/rpc-url")
        .withBodyIncluding("0xa3869e14")
        // 0x1234 encoded bytes
        .thenSendJsonRpcResult(
          "0x000000000000000000000000000000000000000000000000000000000000002000000000000000000000000000000000000000000000000000000000000000021234000000000000000000000000000000000000000000000000000000000000",
        );

      builder.setFieldValue("test-binding", "10");

      const addOrderCalldata = extractWasmEncodedData<string>(
        await builder.generateAddOrderCalldata(),
      );
      assert.equal(addOrderCalldata.length, 2634);

      let result = builder.getCurrentDeployment();
      const currentDeployment =
        extractWasmEncodedData<OrderBuilderDeploymentCfg>(result);
      assert.deepEqual(currentDeployment.deployment.scenario.bindings, {
        "test-binding": "10",
        "another-binding": "300",
      });
    });

    it("generates add order calldata without entering field value", async () => {
      await mockRainlangRegistryCalls();
      await mockServer
        .forPost("/rpc-url")
        .withBodyIncluding("0x56fb83e9")
        .thenSendJsonRpcResult(`0x${"0".repeat(24) + "1".repeat(40)}`);
      // I_STORE()() call
      await mockServer
        .forPost("/rpc-url")
        .withBodyIncluding("0x251ac32e")
        .thenSendJsonRpcResult(`0x${"0".repeat(24) + "2".repeat(40)}`);
      // I_PARSER() call
      await mockServer
        .forPost("/rpc-url")
        .withBodyIncluding("0xf79693f4")
        .thenSendJsonRpcResult(`0x${"0".repeat(24) + "3".repeat(40)}`);
      // parse2() call
      await mockServer
        .forPost("/rpc-url")
        .withBodyIncluding("0xa3869e14")
        // 0x1234 encoded bytes
        .thenSendJsonRpcResult(
          "0x000000000000000000000000000000000000000000000000000000000000002000000000000000000000000000000000000000000000000000000000000000021234000000000000000000000000000000000000000000000000000000000000",
        );

      const addOrderCalldata = extractWasmEncodedData<string>(
        await builder.generateAddOrderCalldata(),
      );
      assert.equal(addOrderCalldata.length, 2634);

      let result = builder.getCurrentDeployment();
      const currentDeployment =
        extractWasmEncodedData<OrderBuilderDeploymentCfg>(result);
      assert.deepEqual(currentDeployment.deployment.scenario.bindings, {
        "test-binding": "10",
        "another-binding": "300",
      });
    });

    it("should generate multicalldata for deposit and add order with existing vault ids", async () => {
      await mockRainlangRegistryCalls();
      await mockServer
        .forPost("/rpc-url")
        .withBodyIncluding("0x56fb83e9")
        .thenSendJsonRpcResult(`0x${"0".repeat(24) + "1".repeat(40)}`);
      // I_STORE()() call
      await mockServer
        .forPost("/rpc-url")
        .withBodyIncluding("0x251ac32e")
        .thenSendJsonRpcResult(`0x${"0".repeat(24) + "2".repeat(40)}`);
      // I_PARSER() call
      await mockServer
        .forPost("/rpc-url")
        .withBodyIncluding("0xf79693f4")
        .thenSendJsonRpcResult(`0x${"0".repeat(24) + "3".repeat(40)}`);
      // parse2() call
      await mockServer
        .forPost("/rpc-url")
        .withBodyIncluding("0xa3869e14")
        // 0x1234 encoded bytes
        .thenSendJsonRpcResult(
          "0x000000000000000000000000000000000000000000000000000000000000002000000000000000000000000000000000000000000000000000000000000000021234000000000000000000000000000000000000000000000000000000000000",
        );

      await builder.setDeposit("token1", "1000");
      await builder.setDeposit("token2", "5000");

      builder.setFieldValue("test-binding", "0xbeef");

      const calldata = extractWasmEncodedData<string>(
        await builder.generateDepositAndAddOrderCalldatas(),
      );
      assert.equal(calldata.length, 3594);

      let result = builder.getCurrentDeployment();
      const currentDeployment =
        extractWasmEncodedData<OrderBuilderDeploymentCfg>(result);
      assert.deepEqual(currentDeployment.deployment.scenario.bindings, {
        "test-binding": "0xbeef",
        "another-binding": "300",
      });
    });

    it("should generate multicalldata for deposit and add order with missing field value", async () => {
      await mockRainlangRegistryCalls();
      await mockServer
        .forPost("/rpc-url")
        .withBodyIncluding("0x56fb83e9")
        .thenSendJsonRpcResult(`0x${"0".repeat(24) + "1".repeat(40)}`);
      // I_STORE()() call
      await mockServer
        .forPost("/rpc-url")
        .withBodyIncluding("0x251ac32e")
        .thenSendJsonRpcResult(`0x${"0".repeat(24) + "2".repeat(40)}`);
      // I_PARSER() call
      await mockServer
        .forPost("/rpc-url")
        .withBodyIncluding("0xf79693f4")
        .thenSendJsonRpcResult(`0x${"0".repeat(24) + "3".repeat(40)}`);
      // parse2() call
      await mockServer
        .forPost("/rpc-url")
        .withBodyIncluding("0xa3869e14")
        // 0x1234 encoded bytes
        .thenSendJsonRpcResult(
          "0x000000000000000000000000000000000000000000000000000000000000002000000000000000000000000000000000000000000000000000000000000000021234000000000000000000000000000000000000000000000000000000000000",
        );

      builder.unsetFieldValue("test-binding");
      assert.deepEqual(
        extractWasmEncodedData<FieldValue[]>(builder.getAllFieldValues()),
        [],
      );

      await builder.setDeposit("token1", "1000");
      await builder.setDeposit("token2", "5000");

      const calldata = extractWasmEncodedData<string>(
        await builder.generateDepositAndAddOrderCalldatas(),
      );
      assert.equal(calldata.length, 3658);

      let result = builder.getCurrentDeployment();
      const currentDeployment =
        extractWasmEncodedData<OrderBuilderDeploymentCfg>(result);
      assert.deepEqual(currentDeployment.deployment.scenario.bindings, {
        "test-binding": "10",
        "another-binding": "300",
      });
    });

    it("should generate multicalldata for deposit and add order with without vault ids", async () => {
      await mockServer
        .forPost("/rpc-url")
        .once()
        .withBodyIncluding("0x82ad56cb")
        .thenSendJsonRpcResult(
          "0x00000000000000000000000000000000000000000000000000000000000000200000000000000000000000000000000000000000000000000000000000000003000000000000000000000000000000000000000000000000000000000000006000000000000000000000000000000000000000000000000000000000000000e000000000000000000000000000000000000000000000000000000000000001a0000000000000000000000000000000000000000000000000000000000000000100000000000000000000000000000000000000000000000000000000000000400000000000000000000000000000000000000000000000000000000000000020000000000000000000000000000000000000000000000000000000000000000600000000000000000000000000000000000000000000000000000000000000010000000000000000000000000000000000000000000000000000000000000040000000000000000000000000000000000000000000000000000000000000006000000000000000000000000000000000000000000000000000000000000000200000000000000000000000000000000000000000000000000000000000000007546f6b656e203100000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000100000000000000000000000000000000000000000000000000000000000000400000000000000000000000000000000000000000000000000000000000000060000000000000000000000000000000000000000000000000000000000000002000000000000000000000000000000000000000000000000000000000000000025431000000000000000000000000000000000000000000000000000000000000",
        );
      await mockServer
        .forPost("/rpc-url")
        .once()
        .withBodyIncluding("0x82ad56cb")
        .thenSendJsonRpcResult(
          "0x00000000000000000000000000000000000000000000000000000000000000200000000000000000000000000000000000000000000000000000000000000003000000000000000000000000000000000000000000000000000000000000006000000000000000000000000000000000000000000000000000000000000000e000000000000000000000000000000000000000000000000000000000000001a000000000000000000000000000000000000000000000000000000000000000010000000000000000000000000000000000000000000000000000000000000040000000000000000000000000000000000000000000000000000000000000002000000000000000000000000000000000000000000000000000000000000000060000000000000000000000000000000000000000000000000000000000000001000000000000000000000000000000000000000000000000000000000000004000000000000000000000000000000000000000000000000000000000000000600000000000000000000000000000000000000000000000000000000000000020000000000000000000000000000000000000000000000000000000000000000754656b656e203200000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000100000000000000000000000000000000000000000000000000000000000000400000000000000000000000000000000000000000000000000000000000000060000000000000000000000000000000000000000000000000000000000000002000000000000000000000000000000000000000000000000000000000000000025432000000000000000000000000000000000000000000000000000000000000",
        );

      let testDotrain = `${builderConfig2}

${dotrainWithoutVaultIds}`;
      let result = await RaindexOrderBuilder.newWithDeployment(
        testDotrain,
        undefined,
        "other-deployment",
      );
      const builder = extractWasmEncodedData(result);

      await mockRainlangRegistryCalls();
      await mockServer
        .forPost("/rpc-url")
        .withBodyIncluding("0x56fb83e9")
        .thenSendJsonRpcResult(`0x${"0".repeat(24) + "1".repeat(40)}`);
      await mockServer
        .forPost("/rpc-url")
        .withBodyIncluding("0x251ac32e")
        .thenSendJsonRpcResult(`0x${"0".repeat(24) + "2".repeat(40)}`);
      await mockServer
        .forPost("/rpc-url")
        .withBodyIncluding("0xf79693f4")
        .thenSendJsonRpcResult(`0x${"0".repeat(24) + "3".repeat(40)}`);
      await mockServer
        .forPost("/rpc-url")
        .withBodyIncluding("0xa3869e14")
        // 0x1234 encoded bytes
        .thenSendJsonRpcResult(
          "0x000000000000000000000000000000000000000000000000000000000000002000000000000000000000000000000000000000000000000000000000000000021234000000000000000000000000000000000000000000000000000000000000",
        );

      await builder.setDeposit("token1", "1000");
      await builder.setDeposit("token2", "5000");

      builder.setFieldValue("test-binding", "0");

      const calldata = extractWasmEncodedData<string>(
        await builder.generateDepositAndAddOrderCalldatas(),
      );
      assert.equal(calldata.length, 3914);

      const currentDeployment =
        extractWasmEncodedData<OrderBuilderDeploymentCfg>(
          builder.getCurrentDeployment(),
        );
      assert.equal(
        currentDeployment.deployment.order.inputs[0].vaultId,
        currentDeployment.deployment.order.outputs[0].vaultId,
      );
    });

    it("should throw error on order operations without selecting required tokens", async () => {
      let testDotrain = `
      ${builderConfig3}

      ${dotrainWithoutTokens}
      `;
      let result = await RaindexOrderBuilder.newWithDeployment(
        testDotrain,
        undefined,
        "other-deployment",
      );
      const testBuilder = extractWasmEncodedData(result);

      let result1 = await testBuilder.checkAllowances(
        "0x1234567890abcdef1234567890abcdef12345678",
      );
      if (result1.error) {
        expect(result1.error.msg).toBe("Token must be selected: token1");
        expect(result1.error.readableMsg).toBe(
          "The token 'token1' must be selected to proceed.",
        );
      } else expect.fail("Expected error");

      let result2 = await testBuilder.generateDepositCalldatas();
      if (result2.error) {
        expect(result2.error.msg).toBe("Token must be selected: token1");
        expect(result2.error.readableMsg).toBe(
          "The token 'token1' must be selected to proceed.",
        );
      } else expect.fail("Expected error");

      let result3 = await testBuilder.generateAddOrderCalldata();
      if (result3.error) {
        expect(result3.error.msg).toBe("Token must be selected: token1");
        expect(result3.error.readableMsg).toBe(
          "The token 'token1' must be selected to proceed.",
        );
      } else expect.fail("Expected error");

      let result4 = await testBuilder.generateDepositAndAddOrderCalldatas();
      if (result4.error) {
        expect(result4.error.msg).toBe("Token must be selected: token1");
        expect(result4.error.readableMsg).toBe(
          "The token 'token1' must be selected to proceed.",
        );
      } else expect.fail("Expected error");
    });

    it("should throw error if field value not set", async () => {
      await mockServer
        .forPost("/rpc-url")
        .once()
        .withBodyIncluding("0x82ad56cb")
        .thenSendJsonRpcResult(
          "0x00000000000000000000000000000000000000000000000000000000000000200000000000000000000000000000000000000000000000000000000000000003000000000000000000000000000000000000000000000000000000000000006000000000000000000000000000000000000000000000000000000000000000e000000000000000000000000000000000000000000000000000000000000001a0000000000000000000000000000000000000000000000000000000000000000100000000000000000000000000000000000000000000000000000000000000400000000000000000000000000000000000000000000000000000000000000020000000000000000000000000000000000000000000000000000000000000000600000000000000000000000000000000000000000000000000000000000000010000000000000000000000000000000000000000000000000000000000000040000000000000000000000000000000000000000000000000000000000000006000000000000000000000000000000000000000000000000000000000000000200000000000000000000000000000000000000000000000000000000000000007546f6b656e203100000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000100000000000000000000000000000000000000000000000000000000000000400000000000000000000000000000000000000000000000000000000000000060000000000000000000000000000000000000000000000000000000000000002000000000000000000000000000000000000000000000000000000000000000025431000000000000000000000000000000000000000000000000000000000000",
        );
      await mockServer
        .forPost("/rpc-url")
        .once()
        .withBodyIncluding("0x82ad56cb")
        .thenSendJsonRpcResult(
          "0x00000000000000000000000000000000000000000000000000000000000000200000000000000000000000000000000000000000000000000000000000000003000000000000000000000000000000000000000000000000000000000000006000000000000000000000000000000000000000000000000000000000000000e000000000000000000000000000000000000000000000000000000000000001a000000000000000000000000000000000000000000000000000000000000000010000000000000000000000000000000000000000000000000000000000000040000000000000000000000000000000000000000000000000000000000000002000000000000000000000000000000000000000000000000000000000000000060000000000000000000000000000000000000000000000000000000000000001000000000000000000000000000000000000000000000000000000000000004000000000000000000000000000000000000000000000000000000000000000600000000000000000000000000000000000000000000000000000000000000020000000000000000000000000000000000000000000000000000000000000000754656b656e203200000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000100000000000000000000000000000000000000000000000000000000000000400000000000000000000000000000000000000000000000000000000000000060000000000000000000000000000000000000000000000000000000000000002000000000000000000000000000000000000000000000000000000000000000025432000000000000000000000000000000000000000000000000000000000000",
        );

      let builderConfig = `
builder:
  name: Test test
  description: Test test test
  deployments:
    other-deployment:
      name: Test test
      description: Test test test
      deposits:
        - token: token1
          min: 0
          presets:
            - "0"
        - token: token2
          min: 0
          presets:
            - "0"
      fields:
        - binding: test-binding
          name: Test binding
          description: Test binding description
          presets:
            - value: "0xbeef"
`;
      let testDotrain = `${builderConfig}

${dotrainWithoutVaultIds}`;
      let result = await RaindexOrderBuilder.newWithDeployment(
        testDotrain,
        undefined,
        "other-deployment",
      );
      const builder = extractWasmEncodedData(result);

      await builder.setDeposit("token1", "1000");
      await builder.setDeposit("token2", "5000");

      let result1 = await builder.generateAddOrderCalldata();
      if (result1.error) {
        expect(result1.error.msg).toBe("Missing field value: Test binding");
        expect(result1.error.readableMsg).toBe(
          "The value for field 'Test binding' is required but has not been set.",
        );
      } else expect.fail("Expected error");

      let result2 = await builder.generateDepositAndAddOrderCalldatas();
      if (result2.error) {
        expect(result2.error.msg).toBe("Missing field value: Test binding");
        expect(result2.error.readableMsg).toBe(
          "The value for field 'Test binding' is required but has not been set.",
        );
      } else expect.fail("Expected error");

      let missingFieldValues = extractWasmEncodedData<
        OrderBuilderFieldDefinitionCfg[]
      >(builder.getMissingFieldValues());
      assert.equal(missingFieldValues.length, 1);
      assert.deepEqual(
        missingFieldValues[0],
        extractWasmEncodedData(builder.getFieldDefinition("test-binding")),
      );
    });

    it("should set vault ids", async () => {
      let stateUpdateCallback = vi.fn();
      mockServer
        .forPost("/rpc-url")
        .withBodyIncluding("0x82ad56cb")
        .thenSendJsonRpcResult(
          "0x00000000000000000000000000000000000000000000000000000000000000200000000000000000000000000000000000000000000000000000000000000003000000000000000000000000000000000000000000000000000000000000006000000000000000000000000000000000000000000000000000000000000000e000000000000000000000000000000000000000000000000000000000000001a0000000000000000000000000000000000000000000000000000000000000000100000000000000000000000000000000000000000000000000000000000000400000000000000000000000000000000000000000000000000000000000000020000000000000000000000000000000000000000000000000000000000000000600000000000000000000000000000000000000000000000000000000000000010000000000000000000000000000000000000000000000000000000000000040000000000000000000000000000000000000000000000000000000000000006000000000000000000000000000000000000000000000000000000000000000200000000000000000000000000000000000000000000000000000000000000007546f6b656e203100000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000100000000000000000000000000000000000000000000000000000000000000400000000000000000000000000000000000000000000000000000000000000060000000000000000000000000000000000000000000000000000000000000002000000000000000000000000000000000000000000000000000000000000000025431000000000000000000000000000000000000000000000000000000000000",
        );

      let testDotrain = `
          ${builderConfig2}

          ${dotrainWithoutVaultIds}
          `;
      let builderResult = await RaindexOrderBuilder.newWithDeployment(
        testDotrain,
        undefined,
        "other-deployment",
        stateUpdateCallback,
      );
      const builder = extractWasmEncodedData(builderResult);

      const currentDeployment =
        extractWasmEncodedData<OrderBuilderDeploymentCfg>(
          builder.getCurrentDeployment(),
        );
      assert.equal(
        currentDeployment.deployment.order.inputs[0].vaultId,
        undefined,
      );
      assert.equal(
        currentDeployment.deployment.order.outputs[0].vaultId,
        undefined,
      );

      assert.equal(
        extractWasmEncodedData<boolean>(builder.hasAnyVaultId()),
        false,
      );

      builder.setVaultId("input", "token1", "0x123");

      assert.equal(
        extractWasmEncodedData<boolean>(builder.hasAnyVaultId()),
        true,
      );

      assert.equal(
        extractWasmEncodedData<Map<string, Map<string, string | undefined>>>(
          builder.getVaultIds(),
        )
          .get("input")
          ?.get("token1"),
        "0x123",
      );
      assert.equal(
        extractWasmEncodedData<Map<string, Map<string, string | undefined>>>(
          builder.getVaultIds(),
        )
          .get("output")
          ?.get("token2"),
        undefined,
      );

      builder.setVaultId("output", "token2", "0x234");

      const newCurrentDeployment =
        extractWasmEncodedData<OrderBuilderDeploymentCfg>(
          builder.getCurrentDeployment(),
        );
      assert.notEqual(
        newCurrentDeployment.deployment.order.inputs[0].vaultId,
        undefined,
      );
      assert.notEqual(
        newCurrentDeployment.deployment.order.outputs[0].vaultId,
        undefined,
      );
      assert.equal(
        newCurrentDeployment.deployment.order.inputs[0].vaultId,
        "0x123",
      );
      assert.equal(
        newCurrentDeployment.deployment.order.outputs[0].vaultId,
        "0x234",
      );

      const vaultIds = extractWasmEncodedData<
        Map<string, Map<string, string | undefined>>
      >(builder.getVaultIds());
      assert.equal(vaultIds.get("input")?.get("token1"), "0x123");
      assert.equal(vaultIds.get("output")?.get("token2"), "0x234");

      builder.setVaultId("input", "token1", undefined);
      assert.equal(
        extractWasmEncodedData<Map<string, Map<string, string | undefined>>>(
          builder.getVaultIds(),
        )
          .get("input")
          ?.get("token1"),
        undefined,
      );

      builder.setVaultId("output", "token2", "");
      assert.equal(
        extractWasmEncodedData<Map<string, Map<string, string | undefined>>>(
          builder.getVaultIds(),
        )
          .get("output")
          ?.get("token2"),
        undefined,
      );

      const result = builder.setVaultId("input", "token1", "test");
      if (!result.error) expect.fail("Expected error");
      expect(result.error.msg).toBe(
        "Invalid value for field 'vault-id': Failed to parse vault id: invalid digit: t in token 'token1' in inputs of order 'some-order'",
      );

      expect(result.error.msg).toContain("token 'token1'");
      expect(result.error.msg).toContain("order 'some-order'");
      expect(result.error.readableMsg).toBe(
        "YAML configuration error: Invalid value for field 'vault-id': Failed to parse vault id: invalid digit: t in token 'token1' in inputs of order 'some-order'",
      );

      assert.equal(stateUpdateCallback.mock.calls.length, 4);
      expect(stateUpdateCallback).toHaveBeenCalledWith(
        extractWasmEncodedData(builder.serializeState()),
      );
    });

    it("should skip deposits with zero amount for deposit calldata", async () => {
      // token1 info
      mockServer
        .forPost("/rpc-url")
        .once()
        .withBodyIncluding("0x82ad56cb")
        .thenSendJsonRpcResult(
          "0x00000000000000000000000000000000000000000000000000000000000000200000000000000000000000000000000000000000000000000000000000000003000000000000000000000000000000000000000000000000000000000000006000000000000000000000000000000000000000000000000000000000000000e000000000000000000000000000000000000000000000000000000000000001a0000000000000000000000000000000000000000000000000000000000000000100000000000000000000000000000000000000000000000000000000000000400000000000000000000000000000000000000000000000000000000000000020000000000000000000000000000000000000000000000000000000000000000600000000000000000000000000000000000000000000000000000000000000010000000000000000000000000000000000000000000000000000000000000040000000000000000000000000000000000000000000000000000000000000006000000000000000000000000000000000000000000000000000000000000000200000000000000000000000000000000000000000000000000000000000000007546f6b656e203100000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000100000000000000000000000000000000000000000000000000000000000000400000000000000000000000000000000000000000000000000000000000000060000000000000000000000000000000000000000000000000000000000000002000000000000000000000000000000000000000000000000000000000000000025431000000000000000000000000000000000000000000000000000000000000",
        );
      // token2 info
      mockServer
        .forPost("/rpc-url")
        .once()
        .withBodyIncluding("0x82ad56cb")
        .thenSendJsonRpcResult(
          "0x00000000000000000000000000000000000000000000000000000000000000200000000000000000000000000000000000000000000000000000000000000003000000000000000000000000000000000000000000000000000000000000006000000000000000000000000000000000000000000000000000000000000000e000000000000000000000000000000000000000000000000000000000000001a000000000000000000000000000000000000000000000000000000000000000010000000000000000000000000000000000000000000000000000000000000040000000000000000000000000000000000000000000000000000000000000002000000000000000000000000000000000000000000000000000000000000000120000000000000000000000000000000000000000000000000000000000000001000000000000000000000000000000000000000000000000000000000000004000000000000000000000000000000000000000000000000000000000000000600000000000000000000000000000000000000000000000000000000000000020000000000000000000000000000000000000000000000000000000000000000754656b656e203200000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000100000000000000000000000000000000000000000000000000000000000000400000000000000000000000000000000000000000000000000000000000000060000000000000000000000000000000000000000000000000000000000000002000000000000000000000000000000000000000000000000000000000000000025432000000000000000000000000000000000000000000000000000000000000",
        );

      await builder.setDeposit("token1", "0");
      await builder.setDeposit("token2", "0");
      const calldatas = extractWasmEncodedData<DepositCalldataResult>(
        await builder.generateDepositCalldatas(),
      );
      // @ts-expect-error - valid result
      assert.equal(calldatas.Calldatas.length, 0);
    });

    it("should generate deployment transaction args", async () => {
      await mockServer
        .forPost("/rpc-url")
        .withBodyIncluding("0x313ce567")
        .once()
        .thenSendJsonRpcResult(
          "0x0000000000000000000000000000000000000000000000000000000000000012",
        );
      await mockServer
        .forPost("/rpc-url")
        .thenSendJsonRpcResult(
          "0x00000000000000000000000000000000000000000000003635C9ADC5DEA00000",
        );
      await mockServer
        .forPost("/metaboard")
        .withBodyIncluding("metaV1S")
        .thenReply(
          200,
          JSON.stringify({
            data: {
              metaV1S: [],
            },
          }),
          { "Content-Type": "application/json" },
        );
      await mockServer
        .forPost("/metaboard")
        .withBodyIncluding("metaBoards")
        .thenReply(
          200,
          JSON.stringify({
            data: {
              metaBoards: [
                { address: "0x0000000000000000000000000000000000000000" },
              ],
            },
          }),
          { "Content-Type": "application/json" },
        );
      await mockServer
        .forPost("/rpc-url")
        .withBodyIncluding("0x56fb83e9")
        .thenSendJsonRpcResult(`0x${"0".repeat(24) + "1".repeat(40)}`);
      await mockServer
        .forPost("/rpc-url")
        .withBodyIncluding("0x251ac32e")
        .thenSendJsonRpcResult(`0x${"0".repeat(24) + "2".repeat(40)}`);
      await mockServer
        .forPost("/rpc-url")
        .withBodyIncluding("0xf79693f4")
        .thenSendJsonRpcResult(`0x${"0".repeat(24) + "3".repeat(40)}`);
      await mockServer
        .forPost("/rpc-url")
        .withBodyIncluding("0xa3869e14")
        .thenSendJsonRpcResult(
          "0x000000000000000000000000000000000000000000000000000000000000002000000000000000000000000000000000000000000000000000000000000000021234000000000000000000000000000000000000000000000000000000000000",
        );

      await builder.setDeposit("token2", "5000");
      builder.setFieldValue("test-binding", "10");

      let result = extractWasmEncodedData<DeploymentTransactionArgs>(
        await builder.getDeploymentTransactionArgs(
          "0x1234567890abcdef1234567890abcdef12345678",
        ),
      );

      assert.equal(result.approvals.length, 1);
      assert.equal(
        result.approvals[0].calldata,
        "0x095ea7b3000000000000000000000000c95a5f8efe14d7a20bd2e5bafec4e71f8ce0b9a600000000000000000000000000000000000000000000010f0cf064dd59200000",
      );
      assert.equal(result.approvals[0].symbol, "T2");
      assert.equal(result.deploymentCalldata.length, 3594);
      assert.equal(
        result.raindexAddress,
        "0xc95a5f8efe14d7a20bd2e5bafec4e71f8ce0b9a6",
      );
      assert.equal(result.chainId, 123);

      const emitMetaCall = result.emitMetaCall;
      assert.ok(emitMetaCall);
      assert.equal(
        emitMetaCall?.to,
        "0x0000000000000000000000000000000000000000",
      );
      const decoded = decodeFunctionData({
        abi: metaBoardAbi,
        data: emitMetaCall!.calldata as `0x${string}`,
      });
      assert.equal(decoded.functionName, "emitMeta");
      const [subject, metaBytes] = decoded.args as [
        `0x${string}`,
        `0x${string}`,
      ];
      assert.match(subject, /^0x[0-9a-f]{64}$/);
      assert.notStrictEqual(subject, `0x${"0".repeat(64)}`);
      assert.ok(metaBytes.startsWith("0xff0a89c674ee7874"));
      const metaText = new TextDecoder().decode(hexToBytes(metaBytes).slice(8));
      assert.ok(metaText.includes("\n#handle-add-order"));

      builder.unsetDeposit("token2");
      result = extractWasmEncodedData<DeploymentTransactionArgs>(
        await builder.getDeploymentTransactionArgs(
          "0x1234567890abcdef1234567890abcdef12345678",
        ),
      );

      assert.equal(result.approvals.length, 0);
      assert.equal(result.deploymentCalldata.length, 2954);
      assert.equal(
        result.raindexAddress,
        "0xc95a5f8efe14d7a20bd2e5bafec4e71f8ce0b9a6",
      );
      assert.equal(result.chainId, 123);

      const emitMetaCallAfterUnset = result.emitMetaCall;
      assert.ok(emitMetaCallAfterUnset);
      assert.equal(
        emitMetaCallAfterUnset?.to,
        "0x0000000000000000000000000000000000000000",
      );
      const decodedAfterUnset = decodeFunctionData({
        abi: metaBoardAbi,
        data: emitMetaCallAfterUnset!.calldata as `0x${string}`,
      });
      assert.equal(decodedAfterUnset.functionName, "emitMeta");
      const [subjectAfterUnset, metaBytesAfterUnset] =
        decodedAfterUnset.args as [`0x${string}`, `0x${string}`];
      assert.match(subjectAfterUnset, /^0x[0-9a-f]{64}$/);
      assert.notStrictEqual(subjectAfterUnset, `0x${"0".repeat(64)}`);
      assert.ok(metaBytesAfterUnset.startsWith("0xff0a89c674ee7874"));
      const metaTextAfterUnset = new TextDecoder().decode(
        hexToBytes(metaBytesAfterUnset).slice(8),
      );
      assert.ok(metaTextAfterUnset.includes("\n#handle-add-order"));
    });
  });

  describe("select tokens tests", async () => {
    let builder: RaindexOrderBuilder;
    let stateUpdateCallback: Mock;

    beforeEach(async () => {
      stateUpdateCallback = vi.fn();
      let dotrain3 = `
      ${builderConfig3}

      ${dotrainWithoutTokens}
      `;
      const result = await RaindexOrderBuilder.newWithDeployment(
        dotrain3,
        undefined,
        "other-deployment",
        stateUpdateCallback,
      );
      builder = extractWasmEncodedData(result);
    });

    it("should get select tokens", async () => {
      const selectTokens = extractWasmEncodedData<
        OrderBuilderSelectTokensCfg[]
      >(builder.getSelectTokens());
      assert.equal(selectTokens.length, 2);
      assert.equal(selectTokens[0].key, "token1");
      assert.equal(selectTokens[1].key, "token2");
    });

    it("should throw error if select tokens not set", async () => {
      mockServer
        .forPost("/rpc-url")
        .once()
        .withBodyIncluding("0x82ad56cb")
        .thenSendJsonRpcResult(
          "0x00000000000000000000000000000000000000000000000000000000000000200000000000000000000000000000000000000000000000000000000000000003000000000000000000000000000000000000000000000000000000000000006000000000000000000000000000000000000000000000000000000000000000e000000000000000000000000000000000000000000000000000000000000001a0000000000000000000000000000000000000000000000000000000000000000100000000000000000000000000000000000000000000000000000000000000400000000000000000000000000000000000000000000000000000000000000020000000000000000000000000000000000000000000000000000000000000000600000000000000000000000000000000000000000000000000000000000000010000000000000000000000000000000000000000000000000000000000000040000000000000000000000000000000000000000000000000000000000000006000000000000000000000000000000000000000000000000000000000000000200000000000000000000000000000000000000000000000000000000000000007546f6b656e203100000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000100000000000000000000000000000000000000000000000000000000000000400000000000000000000000000000000000000000000000000000000000000060000000000000000000000000000000000000000000000000000000000000002000000000000000000000000000000000000000000000000000000000000000025431000000000000000000000000000000000000000000000000000000000000",
        );
      let builderResult = await RaindexOrderBuilder.newWithDeployment(
        dotrainWithBuilder,
        undefined,
        "some-deployment",
      );
      const testBuilder = extractWasmEncodedData(builderResult);

      assert.equal(
        extractWasmEncodedData<OrderBuilderSelectTokensCfg[]>(
          testBuilder.getSelectTokens(),
        ).length,
        0,
      );

      const result = await testBuilder.setSelectToken("token1", "0x1");
      if (!result.error) expect.fail("Expected error");
      expect(result.error.msg).toBe("Select tokens not set");
      expect(result.error.readableMsg).toBe(
        "No tokens have been configured for selection. Please check your YAML configuration.",
      );
    });

    it("should throw error if token not found", async () => {
      const result = await builder.setSelectToken("token3", "0x1");
      if (!result.error) expect.fail("Expected error");
      expect(result.error.msg).toBe("Token not found token3");
      expect(result.error.readableMsg).toBe(
        "The token 'token3' could not be found in the YAML configuration.",
      );
    });

    it("should save select token address", async () => {
      mockServer
        .forPost("/rpc-url")
        .once()
        .thenSendJsonRpcResult(
          "0x00000000000000000000000000000000000000000000000000000000015db63900000000000000000000000000000000000000000000000000000000000000400000000000000000000000000000000000000000000000000000000000000003000000000000000000000000000000000000000000000000000000000000006000000000000000000000000000000000000000000000000000000000000000a000000000000000000000000000000000000000000000000000000000000001200000000000000000000000000000000000000000000000000000000000000020000000000000000000000000000000000000000000000000000000000000000600000000000000000000000000000000000000000000000000000000000000600000000000000000000000000000000000000000000000000000000000000020000000000000000000000000000000000000000000000000000000000000000a54657468657220555344000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000060000000000000000000000000000000000000000000000000000000000000002000000000000000000000000000000000000000000000000000000000000000045553445400000000000000000000000000000000000000000000000000000000",
        );
      // token2 info
      mockServer
        .forPost("/rpc-url")
        .once()
        .thenSendJsonRpcResult(
          "0x00000000000000000000000000000000000000000000000000000000015db63900000000000000000000000000000000000000000000000000000000000000400000000000000000000000000000000000000000000000000000000000000003000000000000000000000000000000000000000000000000000000000000006000000000000000000000000000000000000000000000000000000000000000a000000000000000000000000000000000000000000000000000000000000001200000000000000000000000000000000000000000000000000000000000000020000000000000000000000000000000000000000000000000000000000000000600000000000000000000000000000000000000000000000000000000000000600000000000000000000000000000000000000000000000000000000000000020000000000000000000000000000000000000000000000000000000000000000a54657468657220555344000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000060000000000000000000000000000000000000000000000000000000000000002000000000000000000000000000000000000000000000000000000000000000045553445400000000000000000000000000000000000000000000000000000000",
        );

      assert.equal(
        extractWasmEncodedData<boolean>(builder.isSelectTokenSet("token1")),
        false,
      );
      assert.equal(
        extractWasmEncodedData<boolean>(builder.isSelectTokenSet("token2")),
        false,
      );
      assert.equal(
        extractWasmEncodedData<boolean>(builder.areAllTokensSelected()),
        false,
      );

      let result = await builder.getTokenInfo("token1");
      if (!result.error) expect.fail("Expected error");
      expect(result.error.msg).toBe("Missing required field 'tokens' in root");
      expect(result.error.readableMsg).toBe(
        "YAML configuration error: Missing required field 'tokens' in root",
      );

      result = await builder.getTokenInfo("token2");
      if (!result.error) expect.fail("Expected error");
      expect(result.error.msg).toBe("Missing required field 'tokens' in root");
      expect(result.error.readableMsg).toBe(
        "YAML configuration error: Missing required field 'tokens' in root",
      );

      await builder.setSelectToken(
        "token1",
        "0x6666666666666666666666666666666666666666",
      );
      await builder.setSelectToken(
        "token2",
        "0x8888888888888888888888888888888888888888",
      );

      assert.equal(
        extractWasmEncodedData<boolean>(builder.isSelectTokenSet("token1")),
        true,
      );
      assert.equal(
        extractWasmEncodedData<boolean>(builder.isSelectTokenSet("token2")),
        true,
      );
      assert.equal(
        extractWasmEncodedData<boolean>(builder.areAllTokensSelected()),
        true,
      );

      result = await builder.getTokenInfo("token1");
      const tokenInfo = extractWasmEncodedData<TokenInfo>(result);
      assert.equal(tokenInfo.name, "Tether USD");
      assert.equal(tokenInfo.symbol, "USDT");
      assert.equal(tokenInfo.decimals, 6);

      result = await builder.getTokenInfo("token2");
      const tokenInfo2 = extractWasmEncodedData<TokenInfo>(result);
      assert.equal(tokenInfo2.name, "Tether USD");
      assert.equal(tokenInfo2.symbol, "USDT");
      assert.equal(tokenInfo2.decimals, 6);

      assert.equal(stateUpdateCallback.mock.calls.length, 2);
      expect(stateUpdateCallback).toHaveBeenCalledWith(
        extractWasmEncodedData(builder.serializeState()),
      );
    });

    it("should replace select token", async () => {
      mockServer
        .forPost("/rpc-url")
        .once()
        .thenSendJsonRpcResult(
          "0x00000000000000000000000000000000000000000000000000000000015db63900000000000000000000000000000000000000000000000000000000000000400000000000000000000000000000000000000000000000000000000000000003000000000000000000000000000000000000000000000000000000000000006000000000000000000000000000000000000000000000000000000000000000a000000000000000000000000000000000000000000000000000000000000001200000000000000000000000000000000000000000000000000000000000000020000000000000000000000000000000000000000000000000000000000000000600000000000000000000000000000000000000000000000000000000000000600000000000000000000000000000000000000000000000000000000000000020000000000000000000000000000000000000000000000000000000000000000a54657468657220555344000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000060000000000000000000000000000000000000000000000000000000000000002000000000000000000000000000000000000000000000000000000000000000045553445400000000000000000000000000000000000000000000000000000000",
        );
      mockServer
        .forPost("/rpc-url")
        .once()
        .thenSendJsonRpcResult(
          "0x00000000000000000000000000000000000000000000000000000000015db63900000000000000000000000000000000000000000000000000000000000000400000000000000000000000000000000000000000000000000000000000000003000000000000000000000000000000000000000000000000000000000000006000000000000000000000000000000000000000000000000000000000000000a000000000000000000000000000000000000000000000000000000000000001200000000000000000000000000000000000000000000000000000000000000020000000000000000000000000000000000000000000000000000000000000000600000000000000000000000000000000000000000000000000000000000000600000000000000000000000000000000000000000000000000000000000000020000000000000000000000000000000000000000000000000000000000000000a54657468657220555344000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000060000000000000000000000000000000000000000000000000000000000000002000000000000000000000000000000000000000000000000000000000000000045553445400000000000000000000000000000000000000000000000000000000",
        );

      await builder.setSelectToken(
        "token1",
        "0x6666666666666666666666666666666666666666",
      );
      assert.equal(
        extractWasmEncodedData<boolean>(builder.isSelectTokenSet("token1")),
        true,
      );

      let result = await builder.getTokenInfo("token1");
      const tokenInfo1 = extractWasmEncodedData<TokenInfo>(result);
      assert.equal(tokenInfo1.name, "Tether USD");
      assert.equal(tokenInfo1.symbol, "USDT");
      assert.equal(tokenInfo1.decimals, 6);

      await builder.setSelectToken(
        "token1",
        "0x8888888888888888888888888888888888888888",
      );
      assert.equal(
        extractWasmEncodedData<boolean>(builder.isSelectTokenSet("token1")),
        true,
      );

      result = await builder.getTokenInfo("token1");
      const tokenInfo2 = extractWasmEncodedData<TokenInfo>(result);
      assert.equal(tokenInfo2.name, "Tether USD");
      assert.equal(tokenInfo2.symbol, "USDT");
      assert.equal(tokenInfo2.decimals, 6);

      assert.equal(stateUpdateCallback.mock.calls.length, 2);
      expect(stateUpdateCallback).toHaveBeenCalledWith(
        extractWasmEncodedData(builder.serializeState()),
      );
    });

    it("should remove select token", async () => {
      stateUpdateCallback = vi.fn();
      let dotrain3 = `
      ${builderConfig3}

      ${dotrainWithoutTokens}
      `;
      let result = await RaindexOrderBuilder.newWithDeployment(
        dotrain3,
        undefined,
        "other-deployment",
        stateUpdateCallback,
      );
      const builder = extractWasmEncodedData(result);

      mockServer
        .forPost("/rpc-url")
        .once()
        .thenSendJsonRpcResult(
          "0x00000000000000000000000000000000000000000000000000000000015db63900000000000000000000000000000000000000000000000000000000000000400000000000000000000000000000000000000000000000000000000000000003000000000000000000000000000000000000000000000000000000000000006000000000000000000000000000000000000000000000000000000000000000a000000000000000000000000000000000000000000000000000000000000001200000000000000000000000000000000000000000000000000000000000000020000000000000000000000000000000000000000000000000000000000000000600000000000000000000000000000000000000000000000000000000000000600000000000000000000000000000000000000000000000000000000000000020000000000000000000000000000000000000000000000000000000000000000a54657468657220555344000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000060000000000000000000000000000000000000000000000000000000000000002000000000000000000000000000000000000000000000000000000000000000045553445400000000000000000000000000000000000000000000000000000000",
        );
      mockServer
        .forPost("/rpc-url")
        .once()
        .thenSendJsonRpcResult(
          "0x00000000000000000000000000000000000000000000000000000000015db63900000000000000000000000000000000000000000000000000000000000000400000000000000000000000000000000000000000000000000000000000000003000000000000000000000000000000000000000000000000000000000000006000000000000000000000000000000000000000000000000000000000000000a000000000000000000000000000000000000000000000000000000000000001200000000000000000000000000000000000000000000000000000000000000020000000000000000000000000000000000000000000000000000000000000000600000000000000000000000000000000000000000000000000000000000000600000000000000000000000000000000000000000000000000000000000000020000000000000000000000000000000000000000000000000000000000000000a54657468657220555344000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000060000000000000000000000000000000000000000000000000000000000000002000000000000000000000000000000000000000000000000000000000000000045553445400000000000000000000000000000000000000000000000000000000",
        );

      await builder.setSelectToken(
        "token1",
        "0x6666666666666666666666666666666666666666",
      );
      assert.equal(
        extractWasmEncodedData<boolean>(builder.isSelectTokenSet("token1")),
        true,
      );

      const tokenInfo = extractWasmEncodedData<TokenInfo>(
        await builder.getTokenInfo("token1"),
      );
      assert.equal(tokenInfo.name, "Tether USD");
      assert.equal(tokenInfo.symbol, "USDT");
      assert.equal(tokenInfo.decimals, 6);

      builder.unsetSelectToken("token1");
      assert.equal(
        extractWasmEncodedData<boolean>(builder.isSelectTokenSet("token1")),
        false,
      );

      let result1 = await builder.getTokenInfo("token1");
      if (!result1.error) expect.fail("Expected error");
      expect(result1.error.msg).toBe("Missing required field 'tokens' in root");
      expect(result1.error.readableMsg).toBe(
        "YAML configuration error: Missing required field 'tokens' in root",
      );

      assert.equal(stateUpdateCallback.mock.calls.length, 2);
      expect(stateUpdateCallback).toHaveBeenCalledWith(
        extractWasmEncodedData(builder.serializeState()),
      );
    });

    it("should get all tokens for current network", async () => {
      mockServer
        .forPost("/rpc-url")
        .once()
        .thenSendJsonRpcResult(
          "0x00000000000000000000000000000000000000000000000000000000015db63900000000000000000000000000000000000000000000000000000000000000400000000000000000000000000000000000000000000000000000000000000003000000000000000000000000000000000000000000000000000000000000006000000000000000000000000000000000000000000000000000000000000000a000000000000000000000000000000000000000000000000000000000000001200000000000000000000000000000000000000000000000000000000000000020000000000000000000000000000000000000000000000000000000000000000600000000000000000000000000000000000000000000000000000000000000600000000000000000000000000000000000000000000000000000000000000020000000000000000000000000000000000000000000000000000000000000000a54657468657220555344000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000060000000000000000000000000000000000000000000000000000000000000002000000000000000000000000000000000000000000000000000000000000000045553445400000000000000000000000000000000000000000000000000000000",
        );
      mockServer
        .forPost("/rpc-url")
        .once()
        .thenSendJsonRpcResult(
          "0x00000000000000000000000000000000000000000000000000000000015db63900000000000000000000000000000000000000000000000000000000000000400000000000000000000000000000000000000000000000000000000000000003000000000000000000000000000000000000000000000000000000000000006000000000000000000000000000000000000000000000000000000000000000a000000000000000000000000000000000000000000000000000000000000001200000000000000000000000000000000000000000000000000000000000000020000000000000000000000000000000000000000000000000000000000000000600000000000000000000000000000000000000000000000000000000000000600000000000000000000000000000000000000000000000000000000000000020000000000000000000000000000000000000000000000000000000000000000a54657468657220555344000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000060000000000000000000000000000000000000000000000000000000000000002000000000000000000000000000000000000000000000000000000000000000045553445400000000000000000000000000000000000000000000000000000000",
        );

      await builder.setSelectToken(
        "token1",
        "0x6666666666666666666666666666666666666666",
      );
      await builder.setSelectToken(
        "token2",
        "0x8888888888888888888888888888888888888888",
      );

      const allTokens = extractWasmEncodedData<TokenInfo[]>(
        await builder.getAllTokens(),
      );
      assert.equal(allTokens.length, 2);
      assert.equal(
        allTokens[0].address,
        "0x6666666666666666666666666666666666666666",
      );
      assert.equal(allTokens[0].name, "Tether USD");
      assert.equal(allTokens[0].symbol, "USDT");
      assert.equal(allTokens[0].decimals, 6);
      assert.equal(
        allTokens[1].address,
        "0x8888888888888888888888888888888888888888",
      );
      assert.equal(allTokens[1].name, "Tether USD");
      assert.equal(allTokens[1].symbol, "USDT");
      assert.equal(allTokens[1].decimals, 6);
    });

    it("should get token balance for a given address", async () => {
      // Mock for decimals call
      mockServer
        .forPost("/rpc-url")
        .once()
        .withBodyIncluding("0x313ce567")
        .thenSendJsonRpcResult(
          "0x0000000000000000000000000000000000000000000000000000000000000012",
        );
      // Mock for balanceOf call
      mockServer
        .forPost("/rpc-url")
        .once()
        .withBodyIncluding("0x70a08231")
        .thenSendJsonRpcResult(
          "0x00000000000000000000000000000000000000000000000000000000000003e8",
        );

      const result = extractWasmEncodedData(
        await builder.getAccountBalance(
          "0xc2132d05d31c914a87c6611c10748aeb04b58e8f", // Use token1's address from YAML
          "0x1234567890abcdef1234567890abcdef12345678",
        ),
      );
      assert.equal(result.balance.toFixedDecimal(18).value, BigInt(1000));
      assert.equal(result.formattedBalance, "1e-15");
    });
  });

  describe("remote network tests", () => {
    it("should fetch remote networks", async () => {
      mockServer
        .forGet("/remote-networks")
        .once()
        .thenJson(200, [
          {
            name: "Remote",
            chain: "remote-network",
            chainId: 123,
            rpc: ["http://localhost:8085/rpc-url"],
            networkId: 123,
            nativeCurrency: {
              name: "Remote",
              symbol: "RN",
              decimals: 18,
            },
            infoURL: "http://localhost:8085/info-url",
            shortName: "remote-network",
          },
        ]);
      mockServer
        .forGet("/remote-tokens")
        .once()
        .thenJson(200, {
          name: "Remote",
          timestamp: "2021-01-01T00:00:00.000Z",
          keywords: [],
          version: {
            major: 1,
            minor: 0,
            patch: 0,
          },
          tokens: [],
          logoURI: "http://localhost.com",
        });

      const result = await RaindexOrderBuilder.newWithDeployment(
        dotrainForRemotes,
        undefined,
        "test-deployment",
      );
      const builder = extractWasmEncodedData(result);
      assert.ok(builder.getCurrentDeployment());
    });

    it("should fail for same remote network key in response", async () => {
      mockServer
        .forGet("/remote-networks")
        .once()
        .thenJson(200, [
          {
            name: "Remote",
            chain: "remote-network",
            chainId: 123,
            rpc: ["http://localhost:8085/rpc-url"],
            networkId: 123,
            nativeCurrency: {
              name: "Remote",
              symbol: "RN",
              decimals: 18,
            },
            infoURL: "http://localhost:8085/info-url",
            shortName: "remote-network",
          },
          {
            name: "Remote",
            chain: "remote-network",
            chainId: 123,
            rpc: ["http://localhost:8085/rpc-url"],
            networkId: 123,
            nativeCurrency: {
              name: "Remote",
              symbol: "RN",
              decimals: 18,
            },
            infoURL: "http://localhost:8085/info-url",
            shortName: "remote-network",
          },
        ]);

      const result = await RaindexOrderBuilder.newWithDeployment(
        dotrainForRemotes,
        undefined,
        "test-deployment",
      );
      if (!result.error) expect.fail("Expected error");
      expect(result.error.msg).toBe(
        "Conflicting remote network in response, a network with key 'remote-network' already exists",
      );
      expect(result.error.readableMsg).toBe(
        "Order configuration error in YAML: Conflicting remote network in response, a network with key 'remote-network' already exists",
      );
    });

    it("should fail for duplicate network", async () => {
      mockServer
        .forGet("/remote-networks")
        .once()
        .thenJson(200, [
          {
            name: "Remote",
            chain: "remote-network",
            chainId: 123,
            rpc: ["http://localhost:8085/rpc-url"],
            networkId: 123,
            nativeCurrency: {
              name: "Remote",
              symbol: "RN",
              decimals: 18,
            },
            infoURL: "http://localhost:8085/info-url",
            shortName: "remote-network",
          },
          {
            name: "Some Network",
            chain: "some-network",
            chainId: 999,
            rpc: ["http://localhost:8085/rpc-url"],
            networkId: 999,
            nativeCurrency: {
              name: "Some Network",
              symbol: "ZZ",
              decimals: 18,
            },
            infoURL: "http://localhost:8085/info-url",
            shortName: "some-network",
          },
        ]);
      mockServer
        .forGet("/remote-tokens")
        .once()
        .thenJson(200, {
          name: "Remote",
          timestamp: "2021-01-01T00:00:00.000Z",
          keywords: [],
          version: {
            major: 1,
            minor: 0,
            patch: 0,
          },
          tokens: [],
          logoURI: "http://localhost.com",
        });

      const result = await RaindexOrderBuilder.newWithDeployment(
        dotrainForRemotes,
        undefined,
        "test-deployment",
      );
      if (!result.error) expect.fail("Expected error");
      expect(result.error.msg).toBe(
        "Remote network key shadowing: some-network",
      );
      expect(result.error.readableMsg).toBe(
        "Order configuration error in YAML: Remote network key shadowing: some-network",
      );
    });
  });

  describe("remote tokens tests", () => {
    let builder: RaindexOrderBuilder;

    it("should fetch remote tokens", async () => {
      mockServer
        .forGet("/remote-networks")
        .once()
        .thenJson(200, [
          {
            name: "Remote",
            chain: "remote-network",
            chainId: 123,
            rpc: ["http://localhost:8085/rpc-url"],
            networkId: 123,
            nativeCurrency: {
              name: "Remote",
              symbol: "RN",
              decimals: 18,
            },
            infoURL: "http://localhost:8085/info-url",
            shortName: "remote-network",
          },
        ]);
      mockServer
        .forGet("/remote-tokens")
        .once()
        .thenJson(200, {
          name: "Remote",
          timestamp: "2021-01-01T00:00:00.000Z",
          keywords: [],
          version: {
            major: 1,
            minor: 0,
            patch: 0,
          },
          tokens: [
            {
              chainId: 123,
              address: "0x0000000000000000000000000000000000000000",
              name: "Remote",
              symbol: "RN",
              decimals: 18,
            },
          ],
          logoURI: "http://localhost.com",
        });

      const result = await RaindexOrderBuilder.newWithDeployment(
        dotrainForRemotes,
        undefined,
        "other-deployment",
      );
      const builder = extractWasmEncodedData(result);
      assert.ok(builder.getCurrentDeployment());
    });
  });
});
