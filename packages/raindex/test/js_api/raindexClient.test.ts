import assert from "assert";
import { afterAll, beforeAll, beforeEach, describe, it } from "vitest";
import {
  WasmEncodedResult,
  RaindexClient,
  RaindexOrders,
  SgOrder,
  SgTrade,
  // OrderPerformance, TODO: Issue #1989
  // VaultVolume, TODO: Issue #1989
  SgVault,
  SgTransaction,
  SgAddOrderWithOrder,
  SgRemoveOrderWithOrder,
  Hex,
  Float,
  RaindexYaml,
} from "../../dist/cjs";
import { getLocal } from "mockttp";

const SPEC_VERSION = RaindexYaml.getCurrentSpecVersion().value;

const CHAIN_ID_1_RAINDEX_ADDRESS = "0xc95A5f8eFe14d7a20BD2E5BAFEC4E71f8Ce0B9A6";
const CHAIN_ID_2_RAINDEX_ADDRESS = "0xbeedbeedbeedbeedbeedbeedbeedbeedbeedbeed";
const YAML = `
version: ${SPEC_VERSION}

networks:
    some-network:
        rpcs:
            - http://localhost:8230/rpc1
        chain-id: 1
        network-id: 1
        currency: ETH
    other-network:
        rpcs:
            - http://localhost:8230/rpc2
        chain-id: 2
        network-id: 2
        currency: ETH
subgraphs:
    some-sg: http://localhost:8230/sg1
    other-sg: http://localhost:8230/sg2
metaboards:
    test: https://metaboard.com
rainlangs:
    some-rainlang:
        network: some-network
        address: 0xF14E09601A47552De6aBd3A0B165607FaFd2B5Ba
accounts:
    alice: 0x742d35Cc6634C0532925a3b8D4Fd2d3dB2d4D7fA
    bob: 0x8ba1f109551bD432803012645aac136c0c8D2e80

raindexes:
    some-raindex:
        address: ${CHAIN_ID_1_RAINDEX_ADDRESS}
        network: some-network
        subgraph: some-sg
        local-db-remote: remote
        deployment-block: 12345
    other-raindex:
        address: ${CHAIN_ID_2_RAINDEX_ADDRESS}
        deployment-block: 12345
        network: other-network
        subgraph: other-sg
        local-db-remote: remote
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
        rainlang: some-rainlang
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
      rainlang: some-rainlang
      raindex: some-raindex
deployments:
    some-deployment:
        scenario: some-scenario
        order: some-order
    other-deployment:
        scenario: some-scenario.sub-scenario
        order: some-order
`;

const BYTES32_ZERO = `0x${"0".repeat(64)}`;
const BYTES32_0123 = `0x${"0".repeat(60)}0123`;
const BYTES32_0234 = `0x${"0".repeat(60)}0234`;

const extractWasmEncodedData = <T>(
  result: WasmEncodedResult<T>,
  errorMessage?: string,
): T => {
  if (result.error) {
    assert.fail(errorMessage ?? result.error.msg);
  }
  if (typeof void 0 === typeof result.value) {
    return result.value as T;
  }
  return result.value;
};

describe("Rain Raindex JS API Package Bindgen Tests - Raindex Client", async function () {
  const mockServer = getLocal();
  beforeAll(async () => {
    await mockServer.start(8230);
  });
  afterAll(async () => {
    await mockServer.stop();
  });
  beforeEach(() => {
    mockServer.reset();
  });

  it("should create a new raindex client object", async function () {
    const raindexClient = await RaindexClient.new([YAML]);
    assert.ok(extractWasmEncodedData(raindexClient));
  });

  describe("Orders", async function () {
    const order1: SgOrder = {
      id: BYTES32_0123,
      orderBytes:
        "0x000000000000000000000000000000000000000000000000000000000000002000000000000000000000000005f6c104ca9812ef91fe2e26a2e7187b92d3b0e800000000000000000000000000000000000000000000000000000000000000a000000000000000000000000000000000000000000000000000000000000001a0000000000000000000000000000000000000000000000000000000000000022009cd210f509c66e18fab61fd30f76fb17c6c6cd09f0972ce0815b5b7630a1b050000000000000000000000005fb33d710f8b58de4c9fdec703b5c2487a5219d600000000000000000000000084c6e7f5a1e5dd89594cc25bef4722a1b8871ae600000000000000000000000000000000000000000000000000000000000000600000000000000000000000000000000000000000000000000000000000000075000000000000000000000000000000000000000000000000000000000000000100000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000015020000000c02020002011000000110000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000010000000000000000000000001d80c49bbbcd1c0911346656b529df9e5c2f783d0000000000000000000000000000000000000000000000000000000000000012f5bb1bfe104d351d99dcce1ccfb041ff244a2d3aaf83bd5c4f3fe20b3fceb372000000000000000000000000000000000000000000000000000000000000000100000000000000000000000012e605bc104e93b45e1ad99f9e555f659051c2bb0000000000000000000000000000000000000000000000000000000000000012f5bb1bfe104d351d99dcce1ccfb041ff244a2d3aaf83bd5c4f3fe20b3fceb372",
      orderHash: BYTES32_0123,
      owner: "0x0000000000000000000000000000000000000000",
      outputs: [
        {
          id: "0x0000000000000000000000000000000000000001",
          token: {
            id: "0x12e605bc104e93b45e1ad99f9e555f659051c2bb",
            address: "0x12e605bc104e93b45e1ad99f9e555f659051c2bb",
            name: "Staked FLR",
            symbol: "sFLR",
            decimals: "18",
          },
          balance:
            "0x000000000000000000000000000000000000000000000000000000000000000a",
          vaultId: "0x0123",
          owner: "0x0000000000000000000000000000000000000000",
          ordersAsOutput: [],
          ordersAsInput: [],
          balanceChanges: [],
          raindex: {
            id: CHAIN_ID_1_RAINDEX_ADDRESS,
          },
        },
        {
          id: "0x0000000000000000000000000000000000000003",
          token: {
            id: "0x0000000000000000000000000000000000000000",
            address: "0x0000000000000000000000000000000000000000",
            name: "T3",
            symbol: "T3",
            decimals: "0",
          },
          balance:
            "0x000000000000000000000000000000000000000000000000000000000000000b",
          vaultId: "0x0345",
          owner: "0x0000000000000000000000000000000000000000",
          ordersAsOutput: [],
          ordersAsInput: [],
          balanceChanges: [],
          raindex: {
            id: CHAIN_ID_1_RAINDEX_ADDRESS,
          },
        },
      ],
      inputs: [
        {
          id: "0x0000000000000000000000000000000000000002",
          token: {
            id: "0x1d80c49bbbcd1c0911346656b529df9e5c2f783d",
            address: "0x1d80c49bbbcd1c0911346656b529df9e5c2f783d",
            name: "Wrapped FLR",
            symbol: "WFLR",
            decimals: "18",
          },
          balance:
            "0x000000000000000000000000000000000000000000000000000000000000000c",
          vaultId: "0x0234",
          owner: "0x0000000000000000000000000000000000000000",
          ordersAsOutput: [],
          ordersAsInput: [],
          balanceChanges: [],
          raindex: {
            id: CHAIN_ID_1_RAINDEX_ADDRESS,
          },
        },
        {
          id: "0x0000000000000000000000000000000000000003",
          token: {
            id: "0x0000000000000000000000000000000000000000",
            address: "0x0000000000000000000000000000000000000000",
            name: "T3",
            symbol: "T3",
            decimals: "0",
          },
          balance:
            "0x000000000000000000000000000000000000000000000000000000000000000d",
          vaultId: "0x0345",
          owner: "0x0000000000000000000000000000000000000000",
          ordersAsOutput: [],
          ordersAsInput: [],
          balanceChanges: [],
          raindex: {
            id: CHAIN_ID_1_RAINDEX_ADDRESS,
          },
        },
      ],
      active: true,
      addEvents: [
        {
          transaction: {
            blockNumber: "0",
            timestamp: "0",
            id: BYTES32_ZERO,
            from: "0x0000000000000000000000000000000000000000",
          },
        },
      ],
      meta: "0xff0a89c674ee7874a3005902252f2a20302e2063616c63756c6174652d696f202a2f200a7573696e672d776f7264732d66726f6d203078466532343131434461313933443945346538334135633233344337466433323031303138383361430a616d743a203130302c0a696f3a2063616c6c3c323e28293b0a0a2f2a20312e2068616e646c652d696f202a2f200a3a63616c6c3c333e28292c0a3a656e7375726528657175616c2d746f286f75747075742d7661756c742d64656372656173652829203130302920226d7573742074616b652066756c6c20616d6f756e7422293b0a0a2f2a20322e206765742d696f2d726174696f2d6e6f77202a2f200a656c61707365643a2063616c6c3c343e28292c0a696f3a2073617475726174696e672d73756228302e3031373733353620646976286d756c28656c61707365642073756228302e3031373733353620302e30313733383434292920363029293b0a0a2f2a20332e206f6e652d73686f74202a2f200a3a656e737572652869732d7a65726f286765742868617368286f726465722d68617368282920226861732d657865637574656422292929202268617320657865637574656422292c0a3a7365742868617368286f726465722d68617368282920226861732d657865637574656422292031293b0a0a2f2a20342e206765742d656c6170736564202a2f200a5f3a20737562286e6f772829206765742868617368286f726465722d68617368282920226465706c6f792d74696d65222929293b011bff13109e41336ff20278186170706c69636174696f6e2f6f637465742d73747265616d",
      timestampAdded: "0",
      raindex: {
        id: CHAIN_ID_1_RAINDEX_ADDRESS,
      },
      trades: [],
      removeEvents: [],
    } as unknown as SgOrder;

    const order2 = {
      id: BYTES32_0234,
      orderBytes:
        "0x0000000000000000000000000000000000000000000000000000000000000020000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000a0000000000000000000000000000000000000000000000000000000000000012000000000000000000000000000000000000000000000000000000000000001a00000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000060000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000010000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000001000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000",
      orderHash: BYTES32_0234,
      owner: "0x0000000000000000000000000000000000000000",
      outputs: [
        {
          id: "0x0234",
          token: {
            id: "0x0123",
            address: "0x0000000000000000000000000000000000000000",
            name: "T1",
            symbol: "T1",
            decimals: "0",
          },
          balance:
            "0x0000000000000000000000000000000000000000000000000000000000000000",
          vaultId: "0",
          owner: "0x0000000000000000000000000000000000000000",
          ordersAsOutput: [],
          ordersAsInput: [],
          balanceChanges: [],
          raindex: {
            id: CHAIN_ID_2_RAINDEX_ADDRESS,
          },
        },
      ],
      inputs: [
        {
          id: "0x0234",
          token: {
            id: "0x0234",
            address: "0x0000000000000000000000000000000000000000",
            name: "T2",
            symbol: "T2",
            decimals: "0",
          },
          balance:
            "0x0000000000000000000000000000000000000000000000000000000000000000",
          vaultId: "0",
          owner: "0x0000000000000000000000000000000000000000",
          ordersAsOutput: [],
          ordersAsInput: [],
          balanceChanges: [],
          raindex: {
            id: CHAIN_ID_2_RAINDEX_ADDRESS,
          },
        },
      ],
      active: true,
      addEvents: [
        {
          transaction: {
            blockNumber: "0",
            timestamp: "0",
            id: BYTES32_ZERO,
            from: "0x0000000000000000000000000000000000000000",
          },
        },
      ],
      meta: null,
      timestampAdded: "0",
      raindex: {
        id: CHAIN_ID_2_RAINDEX_ADDRESS,
      },
      trades: [],
      removeEvents: [],
    } as unknown as SgOrder;

    const BYTES32_VOL_ORDER = `0x${"0".repeat(60)}3456`;
    const floatZero = (Float.parse("0").value as Float).asHex();
    const float50 = (Float.parse("50").value as Float).asHex();
    const float100 = (Float.parse("100").value as Float).asHex();

    const order3 = {
      id: BYTES32_VOL_ORDER,
      orderBytes:
        "0x000000000000000000000000000000000000000000000000000000000000002000000000000000000000000005f6c104ca9812ef91fe2e26a2e7187b92d3b0e800000000000000000000000000000000000000000000000000000000000000a000000000000000000000000000000000000000000000000000000000000001a0000000000000000000000000000000000000000000000000000000000000022009cd210f509c66e18fab61fd30f76fb17c6c6cd09f0972ce0815b5b7630a1b050000000000000000000000005fb33d710f8b58de4c9fdec703b5c2487a5219d600000000000000000000000084c6e7f5a1e5dd89594cc25bef4722a1b8871ae600000000000000000000000000000000000000000000000000000000000000600000000000000000000000000000000000000000000000000000000000000075000000000000000000000000000000000000000000000000000000000000000100000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000015020000000c02020002011000000110000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000010000000000000000000000001d80c49bbbcd1c0911346656b529df9e5c2f783d0000000000000000000000000000000000000000000000000000000000000012f5bb1bfe104d351d99dcce1ccfb041ff244a2d3aaf83bd5c4f3fe20b3fceb372000000000000000000000000000000000000000000000000000000000000000100000000000000000000000012e605bc104e93b45e1ad99f9e555f659051c2bb0000000000000000000000000000000000000000000000000000000000000012f5bb1bfe104d351d99dcce1ccfb041ff244a2d3aaf83bd5c4f3fe20b3fceb372",
      orderHash: BYTES32_VOL_ORDER,
      owner: "0x0000000000000000000000000000000000000000",
      outputs: [
        {
          id: BYTES32_0123,
          token: {
            id: BYTES32_0123,
            address: "0x1111111111111111111111111111111111111111",
            name: "Token One",
            symbol: "TK1",
            decimals: "18",
          },
          balance: floatZero,
          vaultId: BYTES32_0123,
          owner: "0x0000000000000000000000000000000000000000",
          ordersAsOutput: [],
          ordersAsInput: [],
          balanceChanges: [],
          raindex: {
            id: CHAIN_ID_1_RAINDEX_ADDRESS,
          },
        },
      ],
      inputs: [
        {
          id: BYTES32_0234,
          token: {
            id: BYTES32_0234,
            address: "0x2222222222222222222222222222222222222222",
            name: "Token Two",
            symbol: "TK2",
            decimals: "18",
          },
          balance: floatZero,
          vaultId: BYTES32_0234,
          owner: "0x0000000000000000000000000000000000000000",
          ordersAsOutput: [],
          ordersAsInput: [],
          balanceChanges: [],
          raindex: {
            id: CHAIN_ID_1_RAINDEX_ADDRESS,
          },
        },
      ],
      active: true,
      addEvents: [
        {
          transaction: {
            blockNumber: "0",
            timestamp: "0",
            id: BYTES32_VOL_ORDER,
            from: "0x0000000000000000000000000000000000000000",
          },
        },
      ],
      meta: null,
      timestampAdded: "0",
      raindex: {
        id: CHAIN_ID_1_RAINDEX_ADDRESS,
      },
      trades: [],
      removeEvents: [],
    } as unknown as SgOrder;

    const mockVolumeTradesList: SgTrade[] = [
      {
        id: "0x07db8b3f3e7498f9d4d0e40b98f57c020d3d277516e86023a8200a20464d4895",
        timestamp: "1632000000",
        tradeEvent: {
          id: BYTES32_ZERO,
          __typename: "TakeOrder",
          sender: "0x0000000000000000000000000000000000000000",
          transaction: {
            id: BYTES32_ZERO,
            from: "0x0000000000000000000000000000000000000000",
            timestamp: "1632000000",
            blockNumber: "0",
          },
        },
        outputVaultBalanceChange: {
          amount: float100,
          vault: {
            id: BYTES32_0123,
            vaultId: BYTES32_0123,
            token: {
              id: BYTES32_0123,
              address: "0x1111111111111111111111111111111111111111",
              name: "Token One",
              symbol: "TK1",
              decimals: "18",
            },
          },
          id: "output-change-vol-1",
          __typename: "TradeVaultBalanceChange",
          newVaultBalance: floatZero,
          oldVaultBalance: float100,
          timestamp: "1632000000",
          transaction: {
            id: BYTES32_ZERO,
            from: "0x0000000000000000000000000000000000000000",
            timestamp: "1632000000",
            blockNumber: "0",
          },
          raindex: { id: CHAIN_ID_1_RAINDEX_ADDRESS },
          trade: { tradeEvent: { __typename: "TakeOrder" } },
        },
        order: {
          id: BYTES32_VOL_ORDER,
          orderHash: BYTES32_VOL_ORDER,
          owner: "0x0000000000000000000000000000000000000000",
        },
        inputVaultBalanceChange: {
          amount: float50,
          vault: {
            id: BYTES32_0234,
            vaultId: BYTES32_0234,
            token: {
              id: BYTES32_0234,
              address: "0x2222222222222222222222222222222222222222",
              name: "Token Two",
              symbol: "TK2",
              decimals: "18",
            },
          },
          id: "input-change-vol-1",
          __typename: "TradeVaultBalanceChange",
          newVaultBalance: float50,
          oldVaultBalance: floatZero,
          timestamp: "1632000000",
          transaction: {
            id: BYTES32_ZERO,
            from: "0x0000000000000000000000000000000000000000",
            timestamp: "1632000000",
            blockNumber: "0",
          },
          raindex: { id: CHAIN_ID_1_RAINDEX_ADDRESS },
          trade: { tradeEvent: { __typename: "TakeOrder" } },
        },
        raindex: {
          id: CHAIN_ID_1_RAINDEX_ADDRESS,
        },
      },
    ] as unknown as SgTrade[];

    const mockOrderTradesList: SgTrade[] = [
      {
        id: "0x07db8b3f3e7498f9d4d0e40b98f57c020d3d277516e86023a8200a20464d4894",
        timestamp: "1632000000",
        tradeEvent: {
          id: BYTES32_ZERO,
          __typename: "TakeOrder",
          sender: "0x0000000000000000000000000000000000000000",
          transaction: {
            id: BYTES32_ZERO,
            from: "0x0000000000000000000000000000000000000000",
            timestamp: "1632000000",
            blockNumber: "0",
          },
        },
        outputVaultBalanceChange: {
          amount:
            "0x0000000000000000000000000000000000000000000000000000000000000001",
          vault: {
            id: "0x0123",
            vaultId: "0x0123",
            token: {
              id: "0x0123",
              address: "0x1111111111111111111111111111111111111111",
              name: "Token One",
              symbol: "TK1",
              decimals: "18",
            },
          },
          id: "output-change-1",
          __typename: "TradeVaultBalanceChange",
          newVaultBalance:
            "0x0000000000000000000000000000000000000000000000000000000000000002",
          oldVaultBalance:
            "0x0000000000000000000000000000000000000000000000000000000000000003",
          timestamp: "1632000000",
          transaction: {
            id: BYTES32_ZERO,
            from: "0x0000000000000000000000000000000000000000",
            timestamp: "1632000000",
            blockNumber: "0",
          },
          raindex: { id: CHAIN_ID_1_RAINDEX_ADDRESS },
          trade: { tradeEvent: { __typename: "TakeOrder" } },
        },
        order: {
          id: order1.id,
          orderHash:
            "0x1234567890abcdef1234567890abcdef1234567890abcdef1234567890abcdef",
          owner: "0x0000000000000000000000000000000000000000",
        },
        inputVaultBalanceChange: {
          amount:
            "0x0000000000000000000000000000000000000000000000000000000000000003",
          vault: {
            id: "0x0234",
            vaultId: "0x0234",
            token: {
              id: "0x0234",
              address: "0x2222222222222222222222222222222222222222",
              name: "Token Two",
              symbol: "TK2",
              decimals: "18",
            },
          },
          id: "input-change-1",
          __typename: "TradeVaultBalanceChange",
          newVaultBalance:
            "0x0000000000000000000000000000000000000000000000000000000000000004",
          oldVaultBalance:
            "0x0000000000000000000000000000000000000000000000000000000000000005",
          timestamp: "1632000000",
          transaction: {
            id: BYTES32_ZERO,
            from: "0x0000000000000000000000000000000000000000",
            timestamp: "1632000000",
            blockNumber: "0",
          },
          raindex: { id: CHAIN_ID_1_RAINDEX_ADDRESS },
          trade: { tradeEvent: { __typename: "TakeOrder" } },
        },
        raindex: {
          id: CHAIN_ID_1_RAINDEX_ADDRESS,
        },
      },
    ] as unknown as SgTrade[];

    const mockTrade: SgTrade = {
      id: BYTES32_0123,
      order: {
        id: BYTES32_0123,
        orderHash: BYTES32_0123,
        owner: "0x0000000000000000000000000000000000000000",
      },
      tradeEvent: {
        id: BYTES32_0123,
        __typename: "TakeOrder",
        sender: "0x0000000000000000000000000000000000000000",
        transaction: {
          id: BYTES32_0123,
          from: "0x0000000000000000000000000000000000000000",
          blockNumber: "0",
          timestamp: "0",
        },
      },
      timestamp: "0",
      raindex: {
        id: CHAIN_ID_1_RAINDEX_ADDRESS,
      },
      outputVaultBalanceChange: {
        id: "0x0123",
        __typename: "TradeVaultBalanceChange",
        amount:
          "0x0000000000000000000000000000000000000000000000000000000000000007",
        newVaultBalance:
          "0x0000000000000000000000000000000000000000000000000000000000000008",
        oldVaultBalance:
          "0x0000000000000000000000000000000000000000000000000000000000000009",
        vault: {
          id: "0x0123",
          vaultId: "0x0123",
          token: {
            id: "0x0123",
            address: "0x0000000000000000000000000000000000000000",
            name: "T1",
            symbol: "T1",
            decimals: "18",
          },
        },
        timestamp: "0",
        transaction: {
          id: BYTES32_0123,
          from: "0x0000000000000000000000000000000000000000",
          blockNumber: "0",
          timestamp: "0",
        },
        raindex: {
          id: CHAIN_ID_1_RAINDEX_ADDRESS,
        },
        trade: { tradeEvent: { __typename: "TakeOrder" } },
      },
      inputVaultBalanceChange: {
        id: "0x0234",
        __typename: "TradeVaultBalanceChange",
        amount:
          "0x0000000000000000000000000000000000000000000000000000000000000005",
        newVaultBalance:
          "0x0000000000000000000000000000000000000000000000000000000000000006",
        oldVaultBalance:
          "0x0000000000000000000000000000000000000000000000000000000000000007",
        vault: {
          id: "0x0234",
          vaultId: "0x0234",
          token: {
            id: "0x0234",
            address: "0x0000000000000000000000000000000000000000",
            name: "T2",
            symbol: "T2",
            decimals: "6",
          },
        },
        timestamp: "0",
        transaction: {
          id: BYTES32_0234,
          from: "0x0000000000000000000000000000000000000000",
          blockNumber: "0",
          timestamp: "0",
        },
        raindex: {
          id: CHAIN_ID_1_RAINDEX_ADDRESS,
        },
        trade: { tradeEvent: { __typename: "TakeOrder" } },
      },
    } as unknown as SgTrade;

    it("should get orders", async function () {
      await mockServer
        .forPost("/sg1")
        .thenReply(200, JSON.stringify({ data: { orders: [order1] } }));
      await mockServer
        .forPost("/sg2")
        .thenReply(200, JSON.stringify({ data: { orders: [order2] } }));

      const raindexClient = extractWasmEncodedData(
        await RaindexClient.new([YAML]),
      );

      let result = extractWasmEncodedData(await raindexClient.getOrders());
      assert.equal(result.orders.length, 2);
      assert.equal(result.orders[0].id, order1.id);
      assert.equal(result.orders[1].id, order2.id);
      assert.equal(result.totalCount, 2);

      result = extractWasmEncodedData(await raindexClient.getOrders([1]));
      assert.equal(result.orders.length, 1);
      assert.equal(result.orders[0].id, order1.id);
      assert.equal(result.totalCount, 1);
    });

    it("should get order by hash", async function () {
      await mockServer
        .forPost("/sg1")
        .thenReply(200, JSON.stringify({ data: { orders: [order1] } }));
      const raindexClient = extractWasmEncodedData(
        await RaindexClient.new([YAML]),
      );

      const order = extractWasmEncodedData(
        await raindexClient.getOrderByHash(
          1,
          CHAIN_ID_1_RAINDEX_ADDRESS,
          BYTES32_0123,
        ),
      );
      assert.equal(typeof order, "object");
      assert.equal(order.id, order1.id);
      assert.equal(order.raindex, order1.raindex.id);
      assert.equal(order.owner, order1.owner);
      assert.equal(order.active, order1.active);
      assert.equal(order.timestampAdded, order1.timestampAdded);
      assert.equal(order.meta, order1.meta);
      assert.equal(
        order.rainlang,
        '/* 0. calculate-io */ \nusing-words-from 0xFe2411CDa193D9E4e83A5c234C7Fd320101883aC\namt: 100,\nio: call<2>();\n\n/* 1. handle-io */ \n:call<3>(),\n:ensure(equal-to(output-vault-decrease() 100) "must take full amount");\n\n/* 2. get-io-ratio-now */ \nelapsed: call<4>(),\nio: saturating-sub(0.0177356 div(mul(elapsed sub(0.0177356 0.0173844)) 60));\n\n/* 3. one-shot */ \n:ensure(is-zero(get(hash(order-hash() "has-executed"))) "has executed"),\n:set(hash(order-hash() "has-executed") 1);\n\n/* 4. get-elapsed */ \n_: sub(now() get(hash(order-hash() "deploy-time")));',
      );
      assert.equal(order.inputsList.items.length, 1);
      assert.equal(order.outputsList.items.length, 1);
      assert.equal(order.vaultsList.items.length, 3);

      assert.equal(order.vaultsList.items[0].vaultType, "input");
      assert.equal(order.vaultsList.items[0].vaultId, "0x0234");
      assert.equal(order.vaultsList.items[0].balance.format().value, "12");
      assert.equal(
        order.vaultsList.items[0].token.id,
        "0x1d80c49bbbcd1c0911346656b529df9e5c2f783d",
      );
      assert.equal(
        order.vaultsList.items[0].token.address,
        "0x1D80c49BbBCd1C0911346656B529DF9E5c2F783d",
      );
      assert.equal(order.vaultsList.items[0].token.name, "Wrapped FLR");
      assert.equal(order.vaultsList.items[0].token.symbol, "WFLR");
      assert.equal(order.vaultsList.items[0].token.decimals, BigInt(18));

      assert.equal(order.vaultsList.items[1].vaultType, "output");
      assert.equal(order.vaultsList.items[1].vaultId, "0x0123");
      assert.equal(order.vaultsList.items[1].balance.format().value, "10");
      assert.equal(
        order.vaultsList.items[1].token.id,
        "0x12e605bc104e93b45e1ad99f9e555f659051c2bb",
      );
      assert.equal(
        order.vaultsList.items[1].token.address,
        "0x12e605bc104e93B45e1aD99F9e555f659051c2BB",
      );
      assert.equal(order.vaultsList.items[1].token.name, "Staked FLR");
      assert.equal(order.vaultsList.items[1].token.symbol, "sFLR");
      assert.equal(order.vaultsList.items[1].token.decimals, BigInt(18));

      assert.equal(order.vaultsList.items[2].vaultType, "inputOutput");
      assert.equal(order.vaultsList.items[2].vaultId, "0x0345");
      assert.equal(order.vaultsList.items[2].balance.format().value, "13");
      assert.equal(
        order.vaultsList.items[2].token.id,
        "0x0000000000000000000000000000000000000000",
      );
      assert.equal(
        order.vaultsList.items[2].token.address,
        "0x0000000000000000000000000000000000000000",
      );
      assert.equal(order.vaultsList.items[2].token.name, "T3");
      assert.equal(order.vaultsList.items[2].token.symbol, "T3");
      assert.equal(order.vaultsList.items[2].token.decimals, "0");
    });

    it("should get the total volume for an order", async () => {
      await mockServer
        .forPost("/sg1")
        .once()
        .thenReply(200, JSON.stringify({ data: { orders: [order3] } }));
      await mockServer
        .forPost("/sg1")
        .once()
        .thenReply(
          200,
          JSON.stringify({
            data: {
              trades: mockVolumeTradesList,
            },
          }),
        );
      await mockServer.forPost("/sg1").thenReply(
        200,
        JSON.stringify({
          data: {
            trades: [],
          },
        }),
      );

      const raindexClient = extractWasmEncodedData(
        await RaindexClient.new([YAML]),
      );
      const order = extractWasmEncodedData(
        await raindexClient.getOrderByHash(
          1,
          CHAIN_ID_1_RAINDEX_ADDRESS,
          BYTES32_VOL_ORDER,
        ),
      );
      const result = extractWasmEncodedData(await order.getVaultsVolume());
      assert.equal(result.length, 2);

      assert.equal(result[0].id, BigInt(BYTES32_0234));
      assert.equal(result[0].token.id, BYTES32_0234);
      assert.equal(
        result[0].token.address,
        "0x2222222222222222222222222222222222222222",
      );
      assert.equal(result[0].token.name, "Token Two");
      assert.equal(result[0].token.symbol, "TK2");
      assert.equal(result[0].token.decimals, BigInt(18));
      assert.equal(result[0].details.netVol.format().value, "50");
      assert.equal(result[0].details.formattedNetVol, "50");
      assert.equal(result[0].details.totalIn.format().value, "50");
      assert.equal(result[0].details.formattedTotalIn, "50");
      assert.equal(result[0].details.totalOut.format().value, "0");
      assert.equal(result[0].details.formattedTotalOut, "0");
      assert.equal(result[0].details.totalVol.format().value, "50");
      assert.equal(result[0].details.formattedTotalVol, "50");

      assert.equal(result[1].id, BigInt(BYTES32_0123));
      assert.equal(result[1].token.id, BYTES32_0123);
      assert.equal(
        result[1].token.address,
        "0x1111111111111111111111111111111111111111",
      );
      assert.equal(result[1].token.name, "Token One");
      assert.equal(result[1].token.symbol, "TK1");
      assert.equal(result[1].token.decimals, BigInt(18));
      assert.equal(result[1].details.netVol.format().value, "100");
      assert.equal(result[1].details.formattedNetVol, "100");
      assert.equal(result[1].details.totalIn.format().value, "100");
      assert.equal(result[1].details.formattedTotalIn, "100");
      assert.equal(result[1].details.totalOut.format().value, "0");
      assert.equal(result[1].details.formattedTotalOut, "0");
      assert.equal(result[1].details.totalVol.format().value, "100");
      assert.equal(result[1].details.formattedTotalVol, "100");
    });

    // TODO: Issue #1989
    // it('should calculate order performance metrics given an order id and subgraph', async () => {
    // 	await mockServer
    // 		.forPost('/sg1')
    // 		.once()
    // 		.thenReply(200, JSON.stringify({ data: { orders: [order3] } }));
    // 	await mockServer
    // 		.forPost('/sg1')
    // 		.once()
    // 		.thenReply(200, JSON.stringify({ data: { order: order3 } }));
    // 	await mockServer
    // 		.forPost('/sg1')
    // 		.once()
    // 		.thenReply(
    // 			200,
    // 			JSON.stringify({
    // 				data: {
    // 					trades: mockOrderTradesList
    // 				}
    // 			})
    // 		);
    // 	await mockServer.forPost('/sg1').thenReply(
    // 		200,
    // 		JSON.stringify({
    // 			data: {
    // 				trades: []
    // 			}
    // 		})
    // 	);

    // 	const raindexClient = extractWasmEncodedData(await RaindexClient.new([YAML]));
    // 	const order = extractWasmEncodedData(
    // 		await raindexClient.getOrderByHash(1, CHAIN_ID_1_RAINDEX_ADDRESS, '0x0123')
    // 	);
    // 	const result = await order.getPerformance(BigInt(1632000000), BigInt(1734571449));
    // 	if (result.error) assert.fail('expected to resolve, but failed');

    // 	const expected: OrderPerformance = {
    // 		orderId: '0x0123',
    // 		orderHash: '0x0123',
    // 		raindex: CHAIN_ID_1_RAINDEX_ADDRESS,
    // 		denominatedPerformance: {
    // 			token: {
    // 				id: '0x0234',
    // 				address: '0x2222222222222222222222222222222222222222',
    // 				name: 'Token Two',
    // 				symbol: 'TK2',
    // 				decimals: '18'
    // 			},
    // 			apy: '0x0',
    // 			apyIsNeg: false,
    // 			netVol: '0x0',
    // 			netVolIsNeg: false,
    // 			startingCapital: '0x258'
    // 		},
    // 		startTime: 1632000000,
    // 		endTime: 1734571449,
    // 		inputsVaults: [
    // 			{
    // 				id: '0x0234',
    // 				token: {
    // 					id: '0x0234',
    // 					address: '0x2222222222222222222222222222222222222222',
    // 					name: 'Token Two',
    // 					symbol: 'TK2',
    // 					decimals: '18'
    // 				},
    // 				volDetails: {
    // 					totalIn: '0x2b5e3af16b1880000',
    // 					totalOut: '0x0',
    // 					totalVol: '0x2b5e3af16b1880000',
    // 					netVol: '0x2b5e3af16b1880000'
    // 				},
    // 				apyDetails: {
    // 					startTime: 1632000000,
    // 					endTime: 1734571449,
    // 					netVol: '0x2b5e3af16b1880000',
    // 					capital: '0x96',
    // 					apy: '0x13bce241d361f7aa7687c05aa7a4e5',
    // 					isNeg: false
    // 				}
    // 			}
    // 		],
    // 		outputsVaults: [
    // 			{
    // 				id: '0x0123',
    // 				token: {
    // 					id: '0x0123',
    // 					address: '0x1111111111111111111111111111111111111111',
    // 					name: 'Token One',
    // 					symbol: 'TK1',
    // 					decimals: '18'
    // 				},
    // 				volDetails: {
    // 					totalIn: '0x0',
    // 					totalOut: '0x56bc75e2d63100000',
    // 					totalVol: '0x56bc75e2d63100000',
    // 					netVol: '0x56bc75e2d63100000'
    // 				},
    // 				apyDetails: {
    // 					startTime: 1632000000,
    // 					endTime: 1734571449,
    // 					netVol: '0x56bc75e2d63100000',
    // 					capital: '0x384',
    // 					apy: '0x6944b6b4675fd38d22d401e37e1a1',
    // 					isNeg: true
    // 				}
    // 			}
    // 		]
    // 	};
    // 	assert.deepEqual(result.value, expected);
    // });

    it("should get remove calldata", async () => {
      await mockServer
        .forPost("/sg1")
        .thenReply(200, JSON.stringify({ data: { orders: [order1] } }));
      const raindexClient = extractWasmEncodedData(
        await RaindexClient.new([YAML]),
      );
      const order = extractWasmEncodedData(
        await raindexClient.getOrderByHash(
          1,
          CHAIN_ID_1_RAINDEX_ADDRESS,
          BYTES32_0123,
        ),
      );

      const calldata = extractWasmEncodedData(order.getRemoveCalldata());
      assert.equal(
        calldata,
        "0xdf6db3c2000000000000000000000000000000000000000000000000000000000000004000000000000000000000000000000000000000000000000000000000000002a000000000000000000000000005f6c104ca9812ef91fe2e26a2e7187b92d3b0e800000000000000000000000000000000000000000000000000000000000000a000000000000000000000000000000000000000000000000000000000000001a0000000000000000000000000000000000000000000000000000000000000020009cd210f509c66e18fab61fd30f76fb17c6c6cd09f0972ce0815b5b7630a1b050000000000000000000000005fb33d710f8b58de4c9fdec703b5c2487a5219d600000000000000000000000084c6e7f5a1e5dd89594cc25bef4722a1b8871ae600000000000000000000000000000000000000000000000000000000000000600000000000000000000000000000000000000000000000000000000000000075000000000000000000000000000000000000000000000000000000000000000100000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000015020000000c02020002011000000110000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000010000000000000000000000001d80c49bbbcd1c0911346656b529df9e5c2f783d0000000000000000000000000000000000000000000000000000000000000012000000000000000000000000000000000000000000000000000000000000000100000000000000000000000012e605bc104e93b45e1ad99f9e555f659051c2bb00000000000000000000000000000000000000000000000000000000000000120000000000000000000000000000000000000000000000000000000000000000",
      );
    });

    it("should get order quote", async () => {
      await mockServer
        .forPost("/sg1")
        .thenReply(200, JSON.stringify({ data: { orders: [order1] } }));
      await mockServer.forPost("/rpc1").once().thenSendJsonRpcResult("0x01");
      // OZ Multicall returns `bytes[]` — 1 element whose contents are the
      // ABI-encoded `quote3Return` tuple `(bool exists, Float outputMax, Float ioRatio)` (96 bytes).
      await mockServer
        .forPost("/rpc1")
        .thenSendJsonRpcResult(
          "0x0000000000000000000000000000000000000000000000000000000000000020" +
            "0000000000000000000000000000000000000000000000000000000000000001" +
            "0000000000000000000000000000000000000000000000000000000000000020" +
            "0000000000000000000000000000000000000000000000000000000000000060" +
            "0000000000000000000000000000000000000000000000000000000000000001" +
            "0000000000000000000000000000000000000000000000000000000000000001" +
            "0000000000000000000000000000000000000000000000000000000000000002",
        );

      const raindexClient = extractWasmEncodedData(
        await RaindexClient.new([YAML]),
      );
      const order = extractWasmEncodedData(
        await raindexClient.getOrderByHash(
          1,
          CHAIN_ID_1_RAINDEX_ADDRESS,
          BYTES32_0123,
        ),
      );

      const result = extractWasmEncodedData(await order.getQuotes());

      assert.equal(result.length, 1);
      assert.deepEqual(result, [
        {
          pair: { pairName: "WFLR/sFLR", inputIndex: 0, outputIndex: 0 },
          blockNumber: 1,
          data: {
            ratio:
              "0x0000000000000000000000000000000000000000000000000000000000000002",
            maxInput:
              "0x0000000000000000000000000000000000000000000000000000000000000002",
            maxOutput:
              "0x0000000000000000000000000000000000000000000000000000000000000001",
            inverseRatio:
              "0xffffffbd2f7a53a390f4323b0f54bbbb472fa8c5db448df40000000000000000",

            formattedInverseRatio: "0.5",
            formattedMaxInput: "2",
            formattedMaxOutput: "1",
            formattedRatio: "2",
          },
          success: true,
          error: undefined,
        },
      ]);
    });

    it("should express the order quote max output as a percentage of its output vault balance when the vault matches", async () => {
      // order1's decoded on-chain validOutputs[0] references vaultId 0x12, but
      // its subgraph output vault carries a different vaultId, so the "should
      // get order quote" test above sees no match and the percentage field is
      // absent. Here we point the subgraph output vault at the on-chain vaultId
      // so the percentage is populated from the already-fetched balance.
      const onchainVaultId = "0x12";
      const matchingOrder = JSON.parse(JSON.stringify(order1)) as SgOrder;
      matchingOrder.outputs[0].vaultId = onchainVaultId;

      await mockServer
        .forPost("/sg1")
        .thenReply(200, JSON.stringify({ data: { orders: [matchingOrder] } }));
      await mockServer.forPost("/rpc1").once().thenSendJsonRpcResult("0x01");
      await mockServer
        .forPost("/rpc1")
        .thenSendJsonRpcResult(
          "0x0000000000000000000000000000000000000000000000000000000000000020" +
            "0000000000000000000000000000000000000000000000000000000000000001" +
            "0000000000000000000000000000000000000000000000000000000000000020" +
            "0000000000000000000000000000000000000000000000000000000000000060" +
            "0000000000000000000000000000000000000000000000000000000000000001" +
            "0000000000000000000000000000000000000000000000000000000000000001" +
            "0000000000000000000000000000000000000000000000000000000000000002",
        );

      const raindexClient = extractWasmEncodedData(
        await RaindexClient.new([YAML]),
      );
      const order = extractWasmEncodedData(
        await raindexClient.getOrderByHash(
          1,
          CHAIN_ID_1_RAINDEX_ADDRESS,
          BYTES32_0123,
        ),
      );

      const result = extractWasmEncodedData(await order.getQuotes());
      assert.equal(result.length, 1);
      // maxOutput 1 / output vault balance 10 * 100 = 10%. Asserting the exact
      // value (not just a non-empty string) proves the computed percentage
      // survives the wasm/serde boundary and reaches JS under the camelCase key
      // the UI reads.
      assert.equal(result[0].data?.formattedMaxOutput, "1");
      assert.equal(result[0].data?.formattedMaxOutputAsPercentOfVault, "10");
      // The input side is deliberately not computed (drawdown is an output-side
      // concept), so the field never exists on the wasm boundary.
      assert.equal(
        "formattedMaxInputAsPercentOfVault" in (result[0].data ?? {}),
        false,
      );
    });

    it("should get order quotes batch", async () => {
      await mockServer
        .forPost("/sg1")
        .thenReply(200, JSON.stringify({ data: { orders: [order1] } }));
      await mockServer.forPost("/rpc1").once().thenSendJsonRpcResult("0x01");
      // OZ Multicall `bytes[]` with 2 elements, each a 96-byte `quote3Return`.
      // elem[0] maxOutput=1/ioRatio=2, elem[1] maxOutput=2/ioRatio=1
      await mockServer
        .forPost("/rpc1")
        .thenSendJsonRpcResult(
          "0x0000000000000000000000000000000000000000000000000000000000000020" +
            "0000000000000000000000000000000000000000000000000000000000000002" +
            "0000000000000000000000000000000000000000000000000000000000000040" +
            "00000000000000000000000000000000000000000000000000000000000000c0" +
            "0000000000000000000000000000000000000000000000000000000000000060" +
            "0000000000000000000000000000000000000000000000000000000000000001" +
            "0000000000000000000000000000000000000000000000000000000000000001" +
            "0000000000000000000000000000000000000000000000000000000000000002" +
            "0000000000000000000000000000000000000000000000000000000000000060" +
            "0000000000000000000000000000000000000000000000000000000000000001" +
            "0000000000000000000000000000000000000000000000000000000000000002" +
            "0000000000000000000000000000000000000000000000000000000000000001",
        );

      const raindexClient = extractWasmEncodedData(
        await RaindexClient.new([YAML]),
      );
      const order = extractWasmEncodedData(
        await raindexClient.getOrderByHash(
          1,
          CHAIN_ID_1_RAINDEX_ADDRESS,
          BYTES32_0123,
        ),
      );

      const orders = new RaindexOrders();
      orders.push(order);
      orders.push(order);
      const result = extractWasmEncodedData(
        await raindexClient.getOrderQuotesBatch(orders),
      );

      assert.equal(result.length, 2);
      assert.equal(result[0].length, 1);
      assert.equal(result[1].length, 1);
      assert.deepEqual(result[0], [
        {
          pair: { pairName: "WFLR/sFLR", inputIndex: 0, outputIndex: 0 },
          blockNumber: 1,
          data: {
            maxOutput:
              "0x0000000000000000000000000000000000000000000000000000000000000001",
            formattedMaxOutput: "1",
            maxInput:
              "0x0000000000000000000000000000000000000000000000000000000000000002",
            formattedMaxInput: "2",
            ratio:
              "0x0000000000000000000000000000000000000000000000000000000000000002",
            formattedRatio: "2",
            inverseRatio:
              "0xffffffbd2f7a53a390f4323b0f54bbbb472fa8c5db448df40000000000000000",
            formattedInverseRatio: "0.5",
          },
          success: true,
          error: undefined,
        },
      ]);
      assert.deepEqual(result[1], [
        {
          pair: { pairName: "WFLR/sFLR", inputIndex: 0, outputIndex: 0 },
          blockNumber: 1,
          data: {
            maxOutput:
              "0x0000000000000000000000000000000000000000000000000000000000000002",
            formattedMaxOutput: "2",
            maxInput:
              "0x0000000000000000000000000000000000000000000000000000000000000002",
            formattedMaxInput: "2",
            ratio:
              "0x0000000000000000000000000000000000000000000000000000000000000001",
            formattedRatio: "1",
            inverseRatio:
              "0xffffffbd5ef4a74721e864761ea977768e5f518bb6891be80000000000000000",
            formattedInverseRatio: "1",
          },
          success: true,
          error: undefined,
        },
      ]);
    });

    describe("Trades", async function () {
      it("should get trades for an order", async function () {
        await mockServer
          .forPost("/sg1")
          .once()
          .thenReply(200, JSON.stringify({ data: { orders: [order1] } }));
        await mockServer
          .forPost("/sg1")
          .once()
          .thenReply(200, JSON.stringify({ data: { orders: [order1] } }));
        await mockServer.forPost("/sg1").thenReply(
          200,
          JSON.stringify({
            data: {
              trades: mockOrderTradesList,
            },
          }),
        );

        const raindexClient = extractWasmEncodedData(
          await RaindexClient.new([YAML]),
        );
        const order = extractWasmEncodedData(
          await raindexClient.getOrderByHash(
            1,
            CHAIN_ID_1_RAINDEX_ADDRESS,
            BYTES32_0123,
          ),
        );
        const result = extractWasmEncodedData(await order.getTradesList());
        assert.equal(result.trades.length, 1);
        assert.equal(result.totalCount, 1);
        assert.ok(result.summary);
        assert.equal(result.trades[0].id, mockOrderTradesList[0].id);
        assert.equal(
          result.trades[0].orderHash,
          mockOrderTradesList[0].order.orderHash,
        );
        assert.equal(
          result.trades[0].timestamp,
          BigInt(mockOrderTradesList[0].timestamp),
        );
        assert.equal(
          result.trades[0].raindex.toLowerCase(),
          mockOrderTradesList[0].raindex.id.toLowerCase(),
        );
        assert.equal(
          result.trades[0].outputVaultBalanceChange.amount.asHex(),
          mockOrderTradesList[0].outputVaultBalanceChange.amount,
        );
        assert.equal(
          result.trades[0].outputVaultBalanceChange.vaultId,
          BigInt(mockOrderTradesList[0].outputVaultBalanceChange.vault.vaultId),
        );
        assert.equal(
          result.trades[0].outputVaultBalanceChange.token.id,
          mockOrderTradesList[0].outputVaultBalanceChange.vault.token.id,
        );
        assert.equal(
          result.trades[0].outputVaultBalanceChange.token.address,
          mockOrderTradesList[0].outputVaultBalanceChange.vault.token.address,
        );
        assert.equal(
          result.trades[0].outputVaultBalanceChange.token.name,
          mockOrderTradesList[0].outputVaultBalanceChange.vault.token.name,
        );
        assert.equal(
          result.trades[0].outputVaultBalanceChange.token.symbol,
          mockOrderTradesList[0].outputVaultBalanceChange.vault.token.symbol,
        );
        assert.equal(
          result.trades[0].outputVaultBalanceChange.token.decimals,
          BigInt(
            mockOrderTradesList[0].outputVaultBalanceChange.vault.token
              .decimals ?? 0,
          ),
        );
        assert.equal(
          result.trades[0].inputVaultBalanceChange.amount.asHex(),
          mockOrderTradesList[0].inputVaultBalanceChange.amount,
        );
        assert.equal(
          result.trades[0].inputVaultBalanceChange.vaultId,
          BigInt(mockOrderTradesList[0].inputVaultBalanceChange.vault.vaultId),
        );
        assert.equal(
          result.trades[0].inputVaultBalanceChange.token.id,
          mockOrderTradesList[0].inputVaultBalanceChange.vault.token.id,
        );
        assert.equal(
          result.trades[0].inputVaultBalanceChange.token.address,
          mockOrderTradesList[0].inputVaultBalanceChange.vault.token.address,
        );
        assert.equal(
          result.trades[0].inputVaultBalanceChange.token.name,
          mockOrderTradesList[0].inputVaultBalanceChange.vault.token.name,
        );
        assert.equal(
          result.trades[0].inputVaultBalanceChange.token.symbol,
          mockOrderTradesList[0].inputVaultBalanceChange.vault.token.symbol,
        );
        assert.equal(
          result.trades[0].inputVaultBalanceChange.token.decimals,
          BigInt(
            mockOrderTradesList[0].inputVaultBalanceChange.vault.token
              .decimals ?? 0,
          ),
        );
        assert.equal(
          result.trades[0].transaction.id,
          mockOrderTradesList[0].tradeEvent.transaction.id,
        );
        assert.equal(
          result.trades[0].transaction.from,
          mockOrderTradesList[0].tradeEvent.transaction.from,
        );
        assert.equal(
          result.trades[0].transaction.blockNumber,
          BigInt(mockOrderTradesList[0].tradeEvent.transaction.blockNumber),
        );
        assert.equal(
          result.trades[0].transaction.timestamp,
          BigInt(mockOrderTradesList[0].tradeEvent.transaction.timestamp),
        );
      });

      it("should get trade detail", async function () {
        await mockServer
          .forPost("/sg1")
          .once()
          .thenReply(200, JSON.stringify({ data: { orders: [order1] } }));
        await mockServer
          .forPost("/sg1")
          .thenReply(200, JSON.stringify({ data: { trade: mockTrade } }));

        const raindexClient = extractWasmEncodedData(
          await RaindexClient.new([YAML]),
        );
        const order = extractWasmEncodedData(
          await raindexClient.getOrderByHash(
            1,
            CHAIN_ID_1_RAINDEX_ADDRESS,
            BYTES32_0123,
          ),
        );
        const result = extractWasmEncodedData(
          await order.getTradeDetail(mockTrade.id as Hex),
        );
        assert.equal(result.id, mockTrade.id);
        assert.equal(result.orderHash, mockTrade.order.orderHash);
        assert.equal(
          result.outputVaultBalanceChange.amount,
          mockTrade.outputVaultBalanceChange.amount,
        );
        assert.equal(
          result.outputVaultBalanceChange.vaultId,
          BigInt(mockTrade.outputVaultBalanceChange.vault.vaultId),
        );
        assert.equal(
          result.outputVaultBalanceChange.token.id,
          mockTrade.outputVaultBalanceChange.vault.token.id,
        );
        assert.equal(
          result.outputVaultBalanceChange.token.address,
          mockTrade.outputVaultBalanceChange.vault.token.address,
        );
        assert.equal(
          result.outputVaultBalanceChange.token.name,
          mockTrade.outputVaultBalanceChange.vault.token.name,
        );
        assert.equal(
          result.outputVaultBalanceChange.token.symbol,
          mockTrade.outputVaultBalanceChange.vault.token.symbol,
        );
        assert.equal(
          result.outputVaultBalanceChange.token.decimals,
          BigInt(mockTrade.outputVaultBalanceChange.vault.token.decimals ?? 0),
        );
        assert.equal(
          result.inputVaultBalanceChange.amount,
          mockTrade.inputVaultBalanceChange.amount,
        );
        assert.equal(
          result.inputVaultBalanceChange.vaultId,
          BigInt(mockTrade.inputVaultBalanceChange.vault.vaultId),
        );
        assert.equal(
          result.inputVaultBalanceChange.token.id,
          mockTrade.inputVaultBalanceChange.vault.token.id,
        );
        assert.equal(
          result.inputVaultBalanceChange.token.address,
          mockTrade.inputVaultBalanceChange.vault.token.address,
        );
        assert.equal(
          result.inputVaultBalanceChange.token.name,
          mockTrade.inputVaultBalanceChange.vault.token.name,
        );
        assert.equal(
          result.inputVaultBalanceChange.token.symbol,
          mockTrade.inputVaultBalanceChange.vault.token.symbol,
        );
        assert.equal(
          result.inputVaultBalanceChange.token.decimals,
          BigInt(mockTrade.inputVaultBalanceChange.vault.token.decimals ?? 0),
        );
        assert.equal(
          result.transaction.id,
          mockTrade.tradeEvent.transaction.id,
        );
        assert.equal(
          result.transaction.from,
          mockTrade.tradeEvent.transaction.from,
        );
        assert.equal(
          result.transaction.blockNumber,
          BigInt(mockTrade.tradeEvent.transaction.blockNumber),
        );
        assert.equal(
          result.transaction.timestamp,
          BigInt(mockTrade.tradeEvent.transaction.timestamp),
        );
        assert.equal(result.raindex, mockTrade.raindex.id.toLowerCase());
      });
    });
  });

  describe("Add and remove orders", async function () {
    const mockOrder = {
      transaction: {
        id: "0xb5d715bc74b7a7f2aac8cca544c1c95e209ed4113b82269ac3285142324bc6af",
        from: "0xf08bcbce72f62c95dcb7c07dcb5ed26acfcfbc11",
        blockNumber: "37432554",
        timestamp: "1739448802",
      },
      order: {
        id: "0x1a69eeb7970d3c8d5776493327fb262e31fc880c9cc4a951607418a7963d9fa1",
        orderBytes:
          "0x0000000000000000000000000000000000000000000000000000000000000020000000000000000000000000f08bcbce72f62c95dcb7c07dcb5ed26acfcfbc1100000000000000000000000000000000000000000000000000000000000000a000000000000000000000000000000000000000000000000000000000000005c00000000000000000000000000000000000000000000000000000000000000640392c489ef67afdc348209452c338ea5ba2b6152b936e152f610d05e1a20621a40000000000000000000000005fb33d710f8b58de4c9fdec703b5c2487a5219d600000000000000000000000084c6e7f5a1e5dd89594cc25bef4722a1b8871ae60000000000000000000000000000000000000000000000000000000000000060000000000000000000000000000000000000000000000000000000000000049d000000000000000000000000000000000000000000000000000000000000000f0000000000000000000000000000000000000000000000000de0b6b3a76400000000000000000000000000000000000000000000000000000c7d713b49da0000914d696e20747261646520616d6f756e742e00000000000000000000000000008b616d6f756e742d75736564000000000000000000000000000000000000000000000000000000000000000000000000000000000000000340aad21b3b70000000000000000000000000000000000000000000000000006194049f30f7200000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000b1a2bc2ec500000000000000000000000000000000000000000000000000000e043da6172500008f6c6173742d74726164652d74696d65000000000000000000000000000000008d6c6173742d74726164652d696f0000000000000000000000000000000000008c696e697469616c2d74696d650000000000000000000000000000000000000000000000000000000000000000000000000000000000000006f05b59d3b200000000000000000000000000000000000000000000000000008ac7230489e80000000000000000000000020000915e36ef882941816356bc3718df868054f868ad000000000000000000000000000000000000000000000000000000000000027d0a00000024007400e0015801b401e001f40218025c080500040b20000200100001001000000b120003001000010b110004001000030b0100051305000201100001011000003d120000011000020010000003100404211200001d02000001100003031000010c1200004911000003100404001000012b12000001100003031000010c1200004a0200001a0b00090b1000060b20000700100000001000011b1200001a10000047120000001000001a1000004712000001100000011000002e12000001100005011000042e120000001000053d12000001100004001000042e1200000010000601100005001000032e120000481200011d0b020a0010000001100000011000062713000001100003031000010c12000049110000001000030010000247120000001000010b110008001000050110000700100001201200001f12000001100000011000004712000000100006001000073d120000011000002b12000000100008001000043b120000160901080b1000070b10000901100008001000013d1200001b12000001100006001000013d1200000b100009001000033a120000001000040010000248120001001000000b110008001000053d12000000100006001000042b1200000a0401011a10000001100009031000010c1200004a020000001000000110000a031000010c1200004a020000040200010110000b031000010c120000491100000803000201100009031000010c120000491100000110000a031000010c12000049110000100c01030110000d001000002e1200000110000c3e120000001000010010000100100001001000010010000100100001001000010010000100100001001000013d1a0000020100010210000e3611000000000000000000000000000000000000000000000000000000000000000000000000010000000000000000000000001d80c49bbbcd1c0911346656b529df9e5c2f783d0000000000000000000000000000000000000000000000000000000000000012a6e3c06415539f92823a18ba63e1c0303040c4892970a0d1e3a27663d7583b33000000000000000000000000000000000000000000000000000000000000000100000000000000000000000012e605bc104e93b45e1ad99f9e555f659051c2bb0000000000000000000000000000000000000000000000000000000000000012a6e3c06415539f92823a18ba63e1c0303040c4892970a0d1e3a27663d7583b33",
        orderHash:
          "0x557147dd0daa80d5beff0023fe6a3505469b2b8c4406ce1ab873e1a652572dd4",
        owner: "0xf08bcbce72f62c95dcb7c07dcb5ed26acfcfbc11",
        outputs: [
          {
            id: "0x49f6b665c395c7b975caa2fc167cb5119981bbb86798bcaf3c4570153d09dfcf",
            owner: "0xf08bcbce72f62c95dcb7c07dcb5ed26acfcfbc11",
            vaultId:
              "75486334982066122983501547829219246999490818941767825330875804445439814023987",
            balance:
              "0x0000000000000000000000000000000000000000000000000000000000000007",
            token: {
              id: "0x12e605bc104e93b45e1ad99f9e555f659051c2bb",
              address: "0x12e605bc104e93b45e1ad99f9e555f659051c2bb",
              name: "Staked FLR",
              symbol: "sFLR",
              decimals: "18",
            },
            raindex: {
              id: CHAIN_ID_1_RAINDEX_ADDRESS,
            },
            ordersAsOutput: [
              {
                id: "0x1a69eeb7970d3c8d5776493327fb262e31fc880c9cc4a951607418a7963d9fa1",
                orderHash:
                  "0x557147dd0daa80d5beff0023fe6a3505469b2b8c4406ce1ab873e1a652572dd4",
                active: true,
              },
            ],
            ordersAsInput: [],
            balanceChanges: [
              {
                __typename: "deposit",
                data: {
                  id: "0x1bf9c93f8ac04810e733b61a7d5dabba66fc1a47235e6ab027e76c9758a2a9e8",
                  __typename: "Deposit",
                  amount:
                    "0x0000000000000000000000000000000000000000000000000000000000000008",
                  newVaultBalance:
                    "0x0000000000000000000000000000000000000000000000000000000000000008",
                  oldVaultBalance:
                    "0x0000000000000000000000000000000000000000000000000000000000000000",
                  vault: {
                    id: "0x49f6b665c395c7b975caa2fc167cb5119981bbb86798bcaf3c4570153d09dfcf",
                    vaultId:
                      "75486334982066122983501547829219246999490818941767825330875804445439814023987",
                    token: {
                      id: "0x12e605bc104e93b45e1ad99f9e555f659051c2bb",
                      address: "0x12e605bc104e93b45e1ad99f9e555f659051c2bb",
                      name: "Staked FLR",
                      symbol: "sFLR",
                      decimals: "18",
                    },
                  },
                  timestamp: "1739448802",
                  transaction: {
                    id: "0xb5d715bc74b7a7f2aac8cca544c1c95e209ed4113b82269ac3285142324bc6af",
                    from: "0xf08bcbce72f62c95dcb7c07dcb5ed26acfcfbc11",
                    blockNumber: "37432554",
                    timestamp: "1739448802",
                  },
                  raindex: {
                    id: CHAIN_ID_1_RAINDEX_ADDRESS,
                  },
                },
              },
              {
                __typename: "withdrawal",
                data: {
                  id: "0x252f6727a7a9bf1047cd9764351e9a2514140c5664589b0e5ecc7f9a4c69329c",
                  __typename: "Withdrawal",
                  amount:
                    "0x0000000000000000000000000000000000000000000000000000000000000009",
                  newVaultBalance:
                    "0x0000000000000000000000000000000000000000000000000000000000000007",
                  oldVaultBalance:
                    "0x0000000000000000000000000000000000000000000000000000000000000008",
                  vault: {
                    id: "0x49f6b665c395c7b975caa2fc167cb5119981bbb86798bcaf3c4570153d09dfcf",
                    vaultId:
                      "75486334982066122983501547829219246999490818941767825330875804445439814023987",
                    token: {
                      id: "0x12e605bc104e93b45e1ad99f9e555f659051c2bb",
                      address: "0x12e605bc104e93b45e1ad99f9e555f659051c2bb",
                      name: "Staked FLR",
                      symbol: "sFLR",
                      decimals: "18",
                    },
                  },
                  timestamp: "1739460802",
                  transaction: {
                    id: "0xf4052dcf0a9ef208be249822c002bf656d273b4583e92928066fd8fb0a67c3f0",
                    from: "0xf08bcbce72f62c95dcb7c07dcb5ed26acfcfbc11",
                    blockNumber: "37439233",
                    timestamp: "1739460802",
                  },
                  raindex: {
                    id: CHAIN_ID_1_RAINDEX_ADDRESS,
                  },
                },
              },
              {
                __typename: "withdrawal",
                data: {
                  id: "0x3b272ce8735a1778d584ed2d49532d571a815909b8f89b2d7d2c6744fcf7cb7c",
                  __typename: "Withdrawal",
                  amount:
                    "0x000000000000000000000000000000000000000000000000000000000000000a",
                  newVaultBalance:
                    "0x0000000000000000000000000000000000000000000000000000000000000008",
                  oldVaultBalance:
                    "0x0000000000000000000000000000000000000000000000000000000000000009",
                  vault: {
                    id: "0x49f6b665c395c7b975caa2fc167cb5119981bbb86798bcaf3c4570153d09dfcf",
                    vaultId:
                      "75486334982066122983501547829219246999490818941767825330875804445439814023987",
                    token: {
                      id: "0x12e605bc104e93b45e1ad99f9e555f659051c2bb",
                      address: "0x12e605bc104e93b45e1ad99f9e555f659051c2bb",
                      name: "Staked FLR",
                      symbol: "sFLR",
                      decimals: "18",
                    },
                  },
                  timestamp: "1739460777",
                  transaction: {
                    id: "0xe3e1be9b3e11420de1f1d34f460c14d8688183b78b2dbcfd9b45560b553e451a",
                    from: "0xf08bcbce72f62c95dcb7c07dcb5ed26acfcfbc11",
                    blockNumber: "37439219",
                    timestamp: "1739460777",
                  },
                  raindex: {
                    id: CHAIN_ID_1_RAINDEX_ADDRESS,
                  },
                },
              },
              {
                __typename: "withdrawal",
                data: {
                  id: "0x9d19a7aa2486c2640669eb04c8c4ed3e11073a04767d6dcfc3468ae12f695849",
                  __typename: "Withdrawal",
                  amount:
                    "0x000000000000000000000000000000000000000000000000000000000000000b",
                  newVaultBalance:
                    "0x0000000000000000000000000000000000000000000000000000000000000008",
                  oldVaultBalance:
                    "0x0000000000000000000000000000000000000000000000000000000000000009",
                  vault: {
                    id: "0x49f6b665c395c7b975caa2fc167cb5119981bbb86798bcaf3c4570153d09dfcf",
                    vaultId:
                      "75486334982066122983501547829219246999490818941767825330875804445439814023987",
                    token: {
                      id: "0x12e605bc104e93b45e1ad99f9e555f659051c2bb",
                      address: "0x12e605bc104e93b45e1ad99f9e555f659051c2bb",
                      name: "Staked FLR",
                      symbol: "sFLR",
                      decimals: "18",
                    },
                  },
                  timestamp: "1739460481",
                  transaction: {
                    id: "0x3bf239fb20fed202f04da468cc62d762390ab5f80b67b477565f740277f94df3",
                    from: "0xf08bcbce72f62c95dcb7c07dcb5ed26acfcfbc11",
                    blockNumber: "37439068",
                    timestamp: "1739460481",
                  },
                  raindex: {
                    id: CHAIN_ID_1_RAINDEX_ADDRESS,
                  },
                },
              },
            ],
          },
        ],
        inputs: [
          {
            id: "0x538830b4f8cc03840cea5af799dc532be4363a3ee8f4c6123dbff7a0acc86dac",
            owner: "0xf08bcbce72f62c95dcb7c07dcb5ed26acfcfbc11",
            vaultId:
              "75486334982066122983501547829219246999490818941767825330875804445439814023987",
            balance:
              "0x0000000000000000000000000000000000000000000000000000000000000007",
            token: {
              id: "0x1d80c49bbbcd1c0911346656b529df9e5c2f783d",
              address: "0x1d80c49bbbcd1c0911346656b529df9e5c2f783d",
              name: "Wrapped Flare",
              symbol: "WFLR",
              decimals: "18",
            },
            raindex: {
              id: CHAIN_ID_1_RAINDEX_ADDRESS,
            },
            ordersAsOutput: [],
            ordersAsInput: [
              {
                id: "0x1a69eeb7970d3c8d5776493327fb262e31fc880c9cc4a951607418a7963d9fa1",
                orderHash:
                  "0x557147dd0daa80d5beff0023fe6a3505469b2b8c4406ce1ab873e1a652572dd4",
                active: true,
              },
            ],
            balanceChanges: [
              {
                __typename: "withdrawal",
                data: {
                  id: "0x3c8de8385099c2f7775cb4695af43d7e38863ae9442402d73f70ebf865da1c4c",
                  __typename: "Withdrawal",
                  amount:
                    "0x0000000000000000000000000000000000000000000000000000000000000008",
                  newVaultBalance:
                    "0x0000000000000000000000000000000000000000000000000000000000000007",
                  oldVaultBalance:
                    "0x0000000000000000000000000000000000000000000000000000000000000008",
                  vault: {
                    id: "0x538830b4f8cc03840cea5af799dc532be4363a3ee8f4c6123dbff7a0acc86dac",
                    vaultId:
                      "75486334982066122983501547829219246999490818941767825330875804445439814023987",
                    token: {
                      id: "0x1d80c49bbbcd1c0911346656b529df9e5c2f783d",
                      address: "0x1d80c49bbbcd1c0911346656b529df9e5c2f783d",
                      name: "Wrapped Flare",
                      symbol: "WFLR",
                      decimals: "18",
                    },
                  },
                  timestamp: "1739460781",
                  transaction: {
                    id: "0x6198a5fdf46f37f336bbd8615c18757f3a83ead6ec63ad02d865d46feb284310",
                    from: "0xf08bcbce72f62c95dcb7c07dcb5ed26acfcfbc11",
                    blockNumber: "37439221",
                    timestamp: "1739460781",
                  },
                  raindex: {
                    id: CHAIN_ID_1_RAINDEX_ADDRESS,
                  },
                },
              },
              {
                __typename: "withdrawal",
                data: {
                  id: "0x7616be6722758517786fdcd94549ce0172d7d34fd411b5778ee0667cd1b1bdba",
                  __typename: "Withdrawal",
                  amount:
                    "0x000000000000000000000000000000000000000000000000000000000000000a",
                  newVaultBalance:
                    "0x0000000000000000000000000000000000000000000000000000000000000009",
                  oldVaultBalance:
                    "0x000000000000000000000000000000000000000000000000000000000000000a",
                  vault: {
                    id: "0x538830b4f8cc03840cea5af799dc532be4363a3ee8f4c6123dbff7a0acc86dac",
                    vaultId:
                      "75486334982066122983501547829219246999490818941767825330875804445439814023987",
                    token: {
                      id: "0x1d80c49bbbcd1c0911346656b529df9e5c2f783d",
                      address: "0x1d80c49bbbcd1c0911346656b529df9e5c2f783d",
                      name: "Wrapped Flare",
                      symbol: "WFLR",
                      decimals: "18",
                    },
                  },
                  timestamp: "1739460415",
                  transaction: {
                    id: "0x8562f41d7d4a8af98ed9db1fbb9575f759846edfab0c4310fc2962b93c5eac7d",
                    from: "0xf08bcbce72f62c95dcb7c07dcb5ed26acfcfbc11",
                    blockNumber: "37439034",
                    timestamp: "1739460415",
                  },
                  raindex: {
                    id: CHAIN_ID_1_RAINDEX_ADDRESS,
                  },
                },
              },
              {
                __typename: "withdrawal",
                data: {
                  id: "0x8e0c007bc831906b8b327be965e6aded6f5b8bc4905823b3047dcd2a69f01c83",
                  __typename: "Withdrawal",
                  amount:
                    "0x000000000000000000000000000000000000000000000000000000000000000c",
                  newVaultBalance:
                    "0x000000000000000000000000000000000000000000000000000000000000000b",
                  oldVaultBalance:
                    "0x000000000000000000000000000000000000000000000000000000000000000c",
                  vault: {
                    id: "0x538830b4f8cc03840cea5af799dc532be4363a3ee8f4c6123dbff7a0acc86dac",
                    vaultId:
                      "75486334982066122983501547829219246999490818941767825330875804445439814023987",
                    token: {
                      id: "0x1d80c49bbbcd1c0911346656b529df9e5c2f783d",
                      address: "0x1d80c49bbbcd1c0911346656b529df9e5c2f783d",
                      name: "Wrapped Flare",
                      symbol: "WFLR",
                      decimals: "18",
                    },
                  },
                  timestamp: "1739460627",
                  transaction: {
                    id: "0xb330355574bd73c72d61b102ba7d23a0e07d677cb97e71db4495d0472587649b",
                    from: "0xf08bcbce72f62c95dcb7c07dcb5ed26acfcfbc11",
                    blockNumber: "37439143",
                    timestamp: "1739460627",
                  },
                  raindex: {
                    id: CHAIN_ID_1_RAINDEX_ADDRESS,
                  },
                },
              },
              {
                __typename: "deposit",
                data: {
                  id: "0xcc853bdf3784e8c2e2ac9a43bdc9a2e56cc0d880a10ae8d25c3d675f6d114e74",
                  __typename: "Deposit",
                  amount:
                    "0x000000000000000000000000000000000000000000000000000000000000000d",
                  newVaultBalance:
                    "0x000000000000000000000000000000000000000000000000000000000000000e",
                  oldVaultBalance:
                    "0x000000000000000000000000000000000000000000000000000000000000000f",
                  vault: {
                    id: "0x538830b4f8cc03840cea5af799dc532be4363a3ee8f4c6123dbff7a0acc86dac",
                    vaultId:
                      "75486334982066122983501547829219246999490818941767825330875804445439814023987",
                    token: {
                      id: "0x1d80c49bbbcd1c0911346656b529df9e5c2f783d",
                      address: "0x1d80c49bbbcd1c0911346656b529df9e5c2f783d",
                      name: "Wrapped Flare",
                      symbol: "WFLR",
                      decimals: "18",
                    },
                  },
                  timestamp: "1739460078",
                  transaction: {
                    id: "0x1f628ccbe37c1395b81c25cc1d9bfef6266d9782c093e1c42bab225335fe8ba0",
                    from: "0xf08bcbce72f62c95dcb7c07dcb5ed26acfcfbc11",
                    blockNumber: "37438849",
                    timestamp: "1739460078",
                  },
                  raindex: {
                    id: CHAIN_ID_1_RAINDEX_ADDRESS,
                  },
                },
              },
            ],
          },
        ],
        raindex: {
          id: CHAIN_ID_1_RAINDEX_ADDRESS,
        },
        active: true,
        timestampAdded: "1739448802",
        meta: "0xff0a89c674ee7874a300590a932f2a20302e2063616c63756c6174652d696f202a2f200a7573696e672d776f7264732d66726f6d20307846653234313143446131393344394534653833413563323334433746643332303130313838336143203078393135453336656638383239343138313633353662433337313844663836383035344638363861440a616d6f756e742d65706f6368730a74726164652d65706f6368733a63616c6c3c323e28292c0a6d61782d6f75747075743a2063616c6c3c333e28616d6f756e742d65706f6368732074726164652d65706f636873292c0a696f3a2063616c6c3c343e2874726164652d65706f636873292c0a3a63616c6c3c353e28696f293b0a0a2f2a20312e2068616e646c652d696f202a2f200a6d696e2d616d6f756e743a206d756c283120302e39292c0a3a656e7375726528677265617465722d7468616e2d6f722d657175616c2d746f286f75747075742d7661756c742d64656372656173652829206d696e2d616d6f756e742920224d696e20747261646520616d6f756e742e22292c0a757365643a206765742868617368286f726465722d6861736828292022616d6f756e742d757365642229292c0a3a7365742868617368286f726465722d6861736828292022616d6f756e742d757365642229206164642875736564206f75747075742d7661756c742d6465637265617365282929293b0a0a2f2a20322e206765742d65706f6368202a2f200a696e697469616c2d74696d653a2063616c6c3c363e28292c0a6c6173742d74696d65205f3a2063616c6c3c373e28292c0a6475726174696f6e3a20737562286e6f77282920616e79286c6173742d74696d6520696e697469616c2d74696d6529292c0a746f74616c2d6475726174696f6e3a20737562286e6f77282920696e697469616c2d74696d65292c0a726174696f2d667265657a652d616d6f756e742d65706f6368733a2064697628312031292c0a726174696f2d667265657a652d74726164652d65706f6368733a206d756c28726174696f2d667265657a652d616d6f756e742d65706f63687320646976283630203138303029292c0a616d6f756e742d65706f6368733a2064697628746f74616c2d6475726174696f6e203630292c0a74726164652d65706f6368733a2073617475726174696e672d73756228646976286475726174696f6e20313830302920726174696f2d667265657a652d74726164652d65706f636873293b0a0a2f2a20332e20616d6f756e742d666f722d65706f6368202a2f200a616d6f756e742d65706f6368730a74726164652d65706f6368733a2c0a746f74616c2d617661696c61626c653a206c696e6561722d67726f7774682830203120616d6f756e742d65706f636873292c0a757365643a206765742868617368286f726465722d6861736828292022616d6f756e742d757365642229292c0a756e757365643a2073756228746f74616c2d617661696c61626c652075736564292c0a64656361793a2063616c6c3c383e2874726164652d65706f636873292c0a7368792d64656361793a20657665727928677265617465722d7468616e2874726164652d65706f63687320302e303529206465636179292c0a7661726961626c652d636f6d706f6e656e743a2073756228312031292c0a7461726765742d616d6f756e743a206164642831206d756c287661726961626c652d636f6d706f6e656e74207368792d646563617929292c0a6361707065642d756e757365643a206d696e28756e75736564207461726765742d616d6f756e74293b0a0a2f2a20342e20696f2d666f722d65706f6368202a2f200a65706f63683a2c0a6c6173742d696f3a2063616c6c3c373e28292c0a6d61782d6e6578742d74726164653a20616e79286d756c286c6173742d696f20312e3031292063616c6c3c393e2829292c0a626173656c696e652d6e6578742d74726164653a206d756c286c6173742d696f2030292c0a7265616c2d626173656c696e653a206d617828626173656c696e652d6e6578742d74726164652063616c6c3c393e2829292c0a7661726961626c652d636f6d706f6e656e743a2073617475726174696e672d737562286d61782d6e6578742d7472616465207265616c2d626173656c696e65292c0a61626f76652d626173656c696e653a206d756c287661726961626c652d636f6d706f6e656e742063616c6c3c383e2865706f636829292c0a5f3a20616464287265616c2d626173656c696e652061626f76652d626173656c696e65293b0a0a2f2a20352e207365742d6c6173742d7472616465202a2f200a6c6173742d696f3a2c0a3a7365742868617368286f726465722d68617368282920226c6173742d74726164652d74696d652229206e6f772829292c0a3a7365742868617368286f726465722d68617368282920226c6173742d74726164652d696f2229206c6173742d696f293b0a0a2f2a20362e206765742d696e697469616c2d74696d65202a2f200a5f3a6765742868617368286f726465722d6861736828292022696e697469616c2d74696d652229293b0a0a2f2a20372e206765742d6c6173742d7472616465202a2f200a6c6173742d74696d653a6765742868617368286f726465722d68617368282920226c6173742d74726164652d74696d652229292c0a6c6173742d696f3a6765742868617368286f726465722d68617368282920226c6173742d74726164652d696f2229293b0a0a2f2a20382e2068616c666c696665202a2f200a65706f63683a2c0a2f2a2a0a202a20536872696e6b696e6720746865206d756c7469706c696572206c696b6520746869730a202a207468656e206170706c79696e672069742031302074696d657320616c6c6f777320666f720a202a2062657474657220707265636973696f6e207768656e206d61782d696f2d726174696f0a202a2069732076657279206c617267652c20652e672e207e31653130206f72207e316532302b0a202a0a202a205468697320776f726b7320626563617573652060706f77657260206c6f7365730a202a20707265636973696f6e206f6e20626173652060302e3560207768656e207468650a202a206578706f6e656e74206973206c6172676520616e642063616e206576656e20676f0a202a20746f20603060207768696c652074686520696f2d726174696f206973207374696c6c0a202a206c617267652e2042657474657220746f206b65657020746865206d756c7469706c6965720a202a2068696768657220707265636973696f6e20616e642064726f702074686520696f2d726174696f0a202a20736d6f6f74686c7920666f72206173206c6f6e672061732077652063616e2e0a202a2f0a6d756c7469706c6965723a0a2020706f77657228302e35206469762865706f636820313029292c0a76616c3a0a20206d756c280a202020206d756c7469706c6965720a202020206d756c7469706c6965720a202020206d756c7469706c6965720a202020206d756c7469706c6965720a202020206d756c7469706c6965720a202020206d756c7469706c6965720a202020206d756c7469706c6965720a202020206d756c7469706c6965720a202020206d756c7469706c6965720a202020206d756c7469706c6965720a2020293b0a0a2f2a20392e2073666c722d626173656c696e652d696e76202a2f200a5f3a20696e762873666c722d65786368616e67652d726174652829293b011bff13109e41336ff20278186170706c69636174696f6e2f6f637465742d73747265616d",
        addEvents: [
          {
            transaction: {
              id: "0xb5d715bc74b7a7f2aac8cca544c1c95e209ed4113b82269ac3285142324bc6af",
              from: "0xf08bcbce72f62c95dcb7c07dcb5ed26acfcfbc11",
              blockNumber: "37432554",
              timestamp: "1739448802",
            },
          },
        ],
        trades: [],
        removeEvents: [],
      },
    };
    const mockAddOrder = mockOrder as SgAddOrderWithOrder;
    const mockRemoveOrder = mockOrder as SgRemoveOrderWithOrder;

    it("should fetch add orders for a given transaction", async function () {
      await mockServer
        .forPost("/sg1")
        .thenReply(
          200,
          JSON.stringify({ data: { addOrders: [mockAddOrder] } }),
        );

      const raindexClient = extractWasmEncodedData(
        await RaindexClient.new([YAML]),
      );
      const result = extractWasmEncodedData(
        await raindexClient.getAddOrdersForTransaction(
          1,
          CHAIN_ID_1_RAINDEX_ADDRESS,
          mockOrder.transaction.id,
        ),
      );
      assert.equal(result[0].id, mockAddOrder.order.id);
      assert.equal(result[0].chainId, BigInt(1));
      assert.equal(result[0].raindex, mockAddOrder.order.raindex.id);
      assert.equal(result[0].transaction?.id, mockAddOrder.transaction.id);
    });

    it("should fetch remove orders for a given transaction", async function () {
      await mockServer
        .forPost("/sg1")
        .thenReply(
          200,
          JSON.stringify({ data: { removeOrders: [mockRemoveOrder] } }),
        );

      const raindexClient = extractWasmEncodedData(
        await RaindexClient.new([YAML]),
      );
      const result = extractWasmEncodedData(
        await raindexClient.getRemoveOrdersForTransaction(
          1,
          CHAIN_ID_1_RAINDEX_ADDRESS,
          mockOrder.transaction.id,
        ),
      );
      assert.equal(result[0].id, mockRemoveOrder.order.id);
      assert.equal(result[0].chainId, BigInt(1));
      assert.equal(result[0].raindex, mockRemoveOrder.order.raindex.id);
      assert.equal(result[0].transaction?.id, mockRemoveOrder.transaction.id);
    });
  });

  describe("Vaults", async function () {
    const vault1: SgVault = {
      id: "0x0123",
      owner: "0x0000000000000000000000000000000000000000",
      vaultId: "0x10",
      balance:
        "0xfffffffa000000000000000000000000000000000000000000000000000f4241",
      token: {
        id: "0x0123",
        address: "0x0000000000000000000000000000000000000000",
        name: "Token 1",
        symbol: "TKN1",
        decimals: "18",
      },
      raindex: {
        id: CHAIN_ID_1_RAINDEX_ADDRESS,
      },
      ordersAsOutput: [],
      ordersAsInput: [],
      balanceChanges: [],
    };
    const vault2: SgVault = {
      id: "0x0234",
      owner: "0x0000000000000000000000000000000000000000",
      vaultId: "0x20",
      balance:
        "0xfffffffa000000000000000000000000000000000000000000000000000f4241",
      token: {
        id: "0x0234",
        address: "0x0000000000000000000000000000000000000000",
        name: "Token 2",
        symbol: "TKN2",
        decimals: "18",
      },
      raindex: {
        id: CHAIN_ID_2_RAINDEX_ADDRESS,
      },
      ordersAsOutput: [],
      ordersAsInput: [],
      balanceChanges: [],
    };

    it("should get vaults", async function () {
      await mockServer
        .forPost("/sg1")
        .thenReply(200, JSON.stringify({ data: { vaults: [vault1] } }));
      await mockServer
        .forPost("/sg2")
        .thenReply(200, JSON.stringify({ data: { vaults: [vault2] } }));

      const raindexClient = extractWasmEncodedData(
        await RaindexClient.new([YAML]),
      );
      const vaultsResult = extractWasmEncodedData(
        await raindexClient.getVaults(
          undefined,
          {
            owners: [],
            hideZeroBalance: false,
          },
          1,
          100,
        ),
      );
      const result = vaultsResult.items;

      assert.equal(vaultsResult.page, 1);
      assert.equal(vaultsResult.pageSize, 100);
      assert.equal(vaultsResult.totalItems, 2);
      assert.equal(vaultsResult.hasMore, false);
      assert.equal(result.length, 2);
      assert.equal(result[0].vaultId, BigInt(vault1.vaultId));
      assert.equal(result[0].owner, vault1.owner);
      assert.equal(result[0].balance.format().value, "1.000001");
      assert.equal(result[0].token.id, vault1.token.id);
      assert.equal(result[0].token.address, vault1.token.address);
      assert.equal(result[0].token.name, vault1.token.name);
      assert.equal(result[0].token.symbol, vault1.token.symbol);
      assert.equal(
        result[0].token.decimals,
        BigInt(vault1.token.decimals ?? 0),
      );
      assert.equal(result[1].vaultId, BigInt(vault2.vaultId));
      assert.equal(result[1].owner, vault2.owner);
      assert.equal(result[1].balance.format().value, "1.000001");
      assert.equal(result[1].token.id, vault2.token.id);
      assert.equal(result[1].token.address, vault2.token.address);
      assert.equal(result[1].token.name, vault2.token.name);
      assert.equal(result[1].token.symbol, vault2.token.symbol);
      assert.equal(
        result[1].token.decimals,
        BigInt(vault2.token.decimals ?? 0),
      );
    });

    it("should get vault", async function () {
      await mockServer
        .forPost("/sg1")
        .thenReply(200, JSON.stringify({ data: { vault: vault1 } }));

      const raindexClient = extractWasmEncodedData(
        await RaindexClient.new([YAML]),
      );
      const result = extractWasmEncodedData(
        await raindexClient.getVault(1, CHAIN_ID_1_RAINDEX_ADDRESS, "0x0123"),
      );

      assert.equal(result.vaultId, BigInt(vault1.vaultId));
      assert.equal(result.owner, vault1.owner);
      assert.equal(result.balance.format().value, "1.000001");
      assert.equal(result.token.id, vault1.token.id);
      assert.equal(result.token.address, vault1.token.address);
      assert.equal(result.token.name, vault1.token.name);
      assert.equal(result.token.symbol, vault1.token.symbol);
      assert.equal(result.token.decimals, BigInt(vault1.token.decimals ?? 0));
      assert.equal(result.raindex, vault1.raindex.id);
    });

    it("should fetch vault balance changes", async () => {
      const mockVaultBalanceChanges = [
        {
          id: "0xdepositid0001",
          __typename: "Deposit",
          amount:
            "0x0000000000000000000000000000000000000000000000000000000000000005",
          newVaultBalance:
            "0x0000000000000000000000000000000000000000000000000000000000000005",
          oldVaultBalance:
            "0x0000000000000000000000000000000000000000000000000000000000000000",
          vault: {
            id: "0x166aeed725f0f3ef9fe62f2a9054035756d55e5560b17afa1ae439e9cd362902",
            vaultId: "0x0123",
            token: {
              id: "0x1d80c49bbbcd1c0911346656b529df9e5c2f783d",
              address: "0x1d80c49bbbcd1c0911346656b529df9e5c2f783d",
              name: "Wrapped Flare",
              symbol: "WFLR",
              decimals: "18",
            },
          },
          timestamp: "1734054063",
          transaction: {
            id: "0x85857b5c6d0b277f9e971b6b45cab98720f90b8f24d65df020776d675b71fc22",
            from: "0x7177b9d00bb5dbcaaf069cc63190902763783b09",
            blockNumber: "34407047",
            timestamp: "1734054063",
          },
          raindex: {
            id: CHAIN_ID_1_RAINDEX_ADDRESS,
          },
        },
      ];

      await mockServer
        .forPost("/sg1")
        .once()
        .thenReply(200, JSON.stringify({ data: { vault: vault1 } }));
      await mockServer.forPost("/sg1").thenReply(
        200,
        JSON.stringify({
          data: { vaultBalanceChanges: mockVaultBalanceChanges },
        }),
      );

      const raindexClient = extractWasmEncodedData(
        await RaindexClient.new([YAML]),
      );
      const vault = extractWasmEncodedData(
        await raindexClient.getVault(1, CHAIN_ID_1_RAINDEX_ADDRESS, "0x0123"),
      );
      const result = extractWasmEncodedData(await vault.getBalanceChanges());

      assert.equal(result[0].type, "deposit");
      assert.equal(result[0].amount.format().value, "5");
      assert.equal(result[0].newBalance.format().value, "5");
      assert.equal(result[0].oldBalance.format().value, "0");
      assert.equal(result[0].timestamp, BigInt("1734054063"));
      assert.equal(
        result[0].transaction.id,
        "0x85857b5c6d0b277f9e971b6b45cab98720f90b8f24d65df020776d675b71fc22",
      );
      assert.equal(
        result[0].transaction.from,
        "0x7177b9d00bB5dbcaaF069CC63190902763783b09",
      );
      assert.equal(result[0].transaction.blockNumber, BigInt("34407047"));
      assert.equal(result[0].transaction.timestamp, BigInt("1734054063"));
      assert.equal(result[0].raindex, CHAIN_ID_1_RAINDEX_ADDRESS);
    });

    const order = {
      id: "0x0123",
      orderBytes:
        "0x0000000000000000000000000000000000000000000000000000000000000020000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000a0000000000000000000000000000000000000000000000000000000000000012000000000000000000000000000000000000000000000000000000000000001a00000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000060000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000100000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000",
      orderHash: "0x0123",
      owner: "0x0000000000000000000000000000000000000000",
      outputs: [
        {
          id: "0x0123",
          token: {
            id: "0x0000000000000000000000000000000000000000",
            address: "0x1234567890123456789012345678901234567890",
            name: "T1",
            symbol: "T1",
            decimals: "0",
          },
          balance:
            "0x0000000000000000000000000000000000000000000000000000000000000008",
          vaultId: "0x2523",
          owner: "0x0000000000000000000000000000000000000000",
          ordersAsOutput: [],
          ordersAsInput: [],
          balanceChanges: [],
          raindex: {
            id: CHAIN_ID_1_RAINDEX_ADDRESS,
          },
        },
      ],
      inputs: [
        {
          id: "0x0234",
          token: {
            id: "0x0000000000000000000000000000000000000000",
            address: "0x9876543210987654321098765432109876543210",
            name: "T2",
            symbol: "T2",
            decimals: "0",
          },
          balance:
            "0x0000000000000000000000000000000000000000000000000000000000000009",
          vaultId: "0x0100",
          owner: "0x0000000000000000000000000000000000000000",
          ordersAsOutput: [],
          ordersAsInput: [],
          balanceChanges: [],
          raindex: {
            id: CHAIN_ID_1_RAINDEX_ADDRESS,
          },
        },
      ],
      active: true,
      addEvents: [
        {
          transaction: {
            blockNumber: "0",
            timestamp: "0",
            id: "0x0123",
            from: "0x0000000000000000000000000000000000000000",
          },
        },
      ],
      meta: null,
      timestampAdded: "0",
      raindex: {
        id: CHAIN_ID_1_RAINDEX_ADDRESS,
      },
      trades: [],
      removeEvents: [],
    };

    it("should read allowance for a vault", async () => {
      await mockServer
        .forPost("/sg1")
        .once()
        .thenReply(200, JSON.stringify({ data: { vault: vault1 } }));
      await mockServer.forPost("/rpc1").thenReply(
        200,
        JSON.stringify({
          jsonrpc: "2.0",
          id: 1,
          result:
            "0x0000000000000000000000000000000000000000000000000000000000000064",
        }),
      );

      const raindexClient = extractWasmEncodedData(
        await RaindexClient.new([YAML]),
      );
      const vault = extractWasmEncodedData(
        await raindexClient.getVault(1, CHAIN_ID_1_RAINDEX_ADDRESS, "0x0123"),
      );
      const res = extractWasmEncodedData(await vault.getAllowance());
      assert.equal(res, "0x64");
    });

    it("should generate all calldatas with approval when allowance is insufficient", async () => {
      await mockServer
        .forPost("/sg1")
        .once()
        .thenReply(200, JSON.stringify({ data: { vault: vault1 } }));
      // Allowance is 100, and user wants 600, so approval calldata is included.
      await mockServer.forPost("/rpc1").thenReply(
        200,
        JSON.stringify({
          jsonrpc: "2.0",
          id: 1,
          result:
            "0x0000000000000000000000000000000000000000000000056BC75E2D63100000",
        }),
      );

      const raindexClient = extractWasmEncodedData(
        await RaindexClient.new([YAML]),
      );
      const vault = extractWasmEncodedData(
        await raindexClient.getVault(1, CHAIN_ID_1_RAINDEX_ADDRESS, "0x0123"),
      );

      const res = extractWasmEncodedData(
        await vault.getCalldatas(Float.parse("600").value as Float),
      );
      assert.ok(res.approval?.startsWith("0x"));
      assert.equal(res.approval?.length, 138);
      assert.ok(res.deposit.startsWith("0x"));
      assert.equal(res.deposit.length, 330);
      assert.ok(res.withdraw.startsWith("0x"));
      assert.equal(res.withdraw.length, 330);
    });

    it("should omit approval calldata when allowance is sufficient", async () => {
      await mockServer
        .forPost("/sg1")
        .once()
        .thenReply(200, JSON.stringify({ data: { vault: vault1 } }));
      // Allowance is 100, and user wants 90, so no approval calldata is needed.
      await mockServer.forPost("/rpc1").thenReply(
        200,
        JSON.stringify({
          jsonrpc: "2.0",
          id: 1,
          result:
            "0x0000000000000000000000000000000000000000000000056BC75E2D63100000",
        }),
      );

      const raindexClient = extractWasmEncodedData(
        await RaindexClient.new([YAML]),
      );
      const vault = extractWasmEncodedData(
        await raindexClient.getVault(1, CHAIN_ID_1_RAINDEX_ADDRESS, "0x0123"),
      );

      const res = extractWasmEncodedData(
        await vault.getCalldatas(Float.parse("90").value as Float),
      );
      assert.equal(res.approval, undefined);
      assert.ok(res.deposit.startsWith("0x"));
      assert.ok(res.withdraw.startsWith("0x"));
    });

    it("should reject getCalldatas for invalid amounts", async () => {
      await mockServer
        .forPost("/sg1")
        .once()
        .thenReply(200, JSON.stringify({ data: { vault: vault1 } }));

      const raindexClient = extractWasmEncodedData(
        await RaindexClient.new([YAML]),
      );
      const vault = extractWasmEncodedData(
        await raindexClient.getVault(1, CHAIN_ID_1_RAINDEX_ADDRESS, "0x0123"),
      );

      let res = await vault.getCalldatas(Float.parse("0").value as Float);
      if (!res.error) assert.fail("expected to reject, but resolved");
      assert.equal(res.error.msg, "Zero amount");
      assert.equal(res.error.readableMsg, "Amount cannot be zero");

      res = await vault.getCalldatas(Float.parse("-100").value as Float);
      if (!res.error) assert.fail("expected to reject, but resolved");
      assert.equal(res.error.msg, "Negative amount");
      assert.equal(res.error.readableMsg, "Amount cannot be negative");
    });

    it("should get all vault tokens", async function () {
      const tokens1 = [
        {
          id: "token1",
          address: "0x1d80c49bbbcd1c0911346656b529df9e5c2f783d",
          name: "Token 1",
          symbol: "TKN1",
          decimals: "18",
        },
        {
          id: "token2",
          address: "0x12e605bc104e93b45e1ad99f9e555f659051c2bb",
          name: "Token 2",
          symbol: "TKN2",
          decimals: "18",
        },
      ];

      const tokens2 = [
        {
          id: "token3",
          address: "0x3333333333333333333333333333333333333333",
          name: "Token 3",
          symbol: "TKN3",
          decimals: "6",
        },
      ];

      await mockServer
        .forPost("/sg1")
        .thenReply(200, JSON.stringify({ data: { erc20S: tokens1 } }));
      await mockServer
        .forPost("/sg2")
        .thenReply(200, JSON.stringify({ data: { erc20S: tokens2 } }));

      const raindexClient = extractWasmEncodedData(
        await RaindexClient.new([YAML]),
      );
      const result = extractWasmEncodedData(
        await raindexClient.getAllVaultTokens(),
      );

      assert.equal(result.length, 3);

      assert.equal(result[0].id, "token1");
      assert.equal(result[0].symbol, "TKN1");
      assert.equal(result[0].name, "Token 1");
      assert.equal(result[0].chainId, 1);
      assert.equal(
        result[0].address,
        "0x1d80c49bbbcd1c0911346656b529df9e5c2f783d",
      );
      assert.equal(result[0].decimals, BigInt(18));

      assert.equal(result[1].id, "token2");
      assert.equal(result[1].symbol, "TKN2");
      assert.equal(result[1].name, "Token 2");
      assert.equal(result[1].chainId, 1);
      assert.equal(
        result[1].address,
        "0x12e605bc104e93b45e1ad99f9e555f659051c2bb",
      );

      assert.equal(result[2].id, "token3");
      assert.equal(result[2].symbol, "TKN3");
      assert.equal(result[2].name, "Token 3");
      assert.equal(result[2].chainId, 2);
      assert.equal(
        result[2].address,
        "0x3333333333333333333333333333333333333333",
      );
      assert.equal(result[2].decimals, BigInt(6));
    });

    it("should get all vault tokens with chain filter", async function () {
      const tokens1 = [
        {
          id: "token1",
          address: "0x1d80c49bbbcd1c0911346656b529df9e5c2f783d",
          name: "Token 1",
          symbol: "TKN1",
          decimals: "18",
        },
      ];

      await mockServer
        .forPost("/sg1")
        .thenReply(200, JSON.stringify({ data: { erc20S: tokens1 } }));

      const raindexClient = extractWasmEncodedData(
        await RaindexClient.new([YAML]),
      );
      const result = extractWasmEncodedData(
        await raindexClient.getAllVaultTokens([1]),
      );

      // Should have only 1 token from chain 1
      assert.equal(result.length, 1);
      assert.equal(result[0].id, "token1");
      assert.equal(result[0].chainId, 1);
    });

    it("should fetch account balance from a raindex vault instance", async () => {
      await mockServer
        .forPost("/sg1")
        .once()
        .thenReply(200, JSON.stringify({ data: { vault: vault1 } }));
      await mockServer.forPost("/rpc1").thenReply(
        200,
        JSON.stringify({
          jsonrpc: "2.0",
          id: 1,
          result:
            "0x00000000000000000000000000000000000000000000000000000000000003e8",
        }),
      );

      const raindexClient = extractWasmEncodedData(
        await RaindexClient.new([YAML]),
      );
      const vault = extractWasmEncodedData(
        await raindexClient.getVault(1, CHAIN_ID_1_RAINDEX_ADDRESS, "0x0123"),
      );

      const res = extractWasmEncodedData(await vault.getOwnerBalance());
      assert.equal(res.balance.toFixedDecimal(18).value, BigInt(1000));
      assert.equal(res.formattedBalance, "1e-15");
    });

    it("should fetch an arbitrary account balance from a raindex vault instance", async () => {
      await mockServer
        .forPost("/sg1")
        .once()
        .thenReply(200, JSON.stringify({ data: { vault: vault1 } }));
      await mockServer.forPost("/rpc1").thenReply(
        200,
        JSON.stringify({
          jsonrpc: "2.0",
          id: 1,
          result:
            "0x00000000000000000000000000000000000000000000000000000000000003e8",
        }),
      );

      const raindexClient = extractWasmEncodedData(
        await RaindexClient.new([YAML]),
      );
      const vault = extractWasmEncodedData(
        await raindexClient.getVault(1, CHAIN_ID_1_RAINDEX_ADDRESS, "0x0123"),
      );

      const res = extractWasmEncodedData(
        await vault.getAccountBalance("0x9876543210987654321098765432109876543210"),
      );
      assert.equal(res.balance.toFixedDecimal(18).value, BigInt(1000));
      assert.equal(res.formattedBalance, "1e-15");
    });
  });

  describe("Transactions", () => {
    const transaction = {
      id: BYTES32_0123,
      from: "0x1000000000000000000000000000000000000000",
      blockNumber: "2356",
      timestamp: "1734054063",
    } as SgTransaction;

    it("should get transaction", async () => {
      await mockServer
        .forPost("/sg1")
        .thenReply(200, JSON.stringify({ data: { transaction } }));

      const raindexClient = extractWasmEncodedData(
        await RaindexClient.new([YAML]),
      );
      const result = extractWasmEncodedData(
        await raindexClient.getTransaction(
          1,
          CHAIN_ID_1_RAINDEX_ADDRESS,
          BYTES32_0123,
        ),
      );
      assert.equal(result.id, transaction.id);
      assert.equal(result.from, transaction.from);
      assert.equal(result.blockNumber, BigInt(transaction.blockNumber));
      assert.equal(result.timestamp, BigInt(transaction.timestamp));
    });
  });

  describe("Raindex yaml", () => {
    it("should get unique chain ids", async () => {
      const raindexClient = extractWasmEncodedData(
        await RaindexClient.new([YAML]),
      );
      const result = extractWasmEncodedData(raindexClient.getUniqueChainIds());
      assert.equal(result.length, 2);
      assert(result.includes(1));
      assert(result.includes(2));
    });

    it("should get all networks", async () => {
      const raindexClient = extractWasmEncodedData(
        await RaindexClient.new([YAML]),
      );
      const result = extractWasmEncodedData(raindexClient.getAllNetworks());
      assert.equal(result.size, 2);
      assert.equal(result.get("some-network")?.chainId, 1);
      assert.equal(result.get("other-network")?.chainId, 2);
    });

    it("should get network by chain id", async () => {
      const raindexClient = extractWasmEncodedData(
        await RaindexClient.new([YAML]),
      );
      const result = extractWasmEncodedData(
        raindexClient.getNetworkByChainId(1),
      );
      assert.equal(result.chainId, 1);
    });

    it("should get raindex by address", async () => {
      const raindexClient = extractWasmEncodedData(
        await RaindexClient.new([YAML]),
      );

      let result = extractWasmEncodedData(
        raindexClient.getRaindexByAddress(CHAIN_ID_1_RAINDEX_ADDRESS),
      );
      assert.equal(
        result.address.toLowerCase(),
        CHAIN_ID_1_RAINDEX_ADDRESS.toLowerCase(),
      );

      result = extractWasmEncodedData(
        raindexClient.getRaindexByAddress(CHAIN_ID_2_RAINDEX_ADDRESS),
      );
      assert.equal(
        result.address.toLowerCase(),
        CHAIN_ID_2_RAINDEX_ADDRESS.toLowerCase(),
      );
    });

    it("should check if sentry is enabled", async () => {
      const raindexClient = extractWasmEncodedData(
        await RaindexClient.new([YAML]),
      );
      const result = extractWasmEncodedData(raindexClient.isSentryEnabled());
      assert.equal(result, false);
    });

    it("should get all accounts", async () => {
      const raindexClient = extractWasmEncodedData(
        await RaindexClient.new([YAML]),
      );
      const result = extractWasmEncodedData(raindexClient.getAllAccounts());
      assert.equal(result.size, 2);
      assert(result.has("alice"));
      assert(result.has("bob"));
      assert.equal(
        result.get("alice")?.address.toLowerCase(),
        "0x742d35cc6634c0532925a3b8d4fd2d3db2d4d7fa",
      );
      assert.equal(
        result.get("bob")?.address.toLowerCase(),
        "0x8ba1f109551bd432803012645aac136c0c8d2e80",
      );
    });

    it("should get all tokens", async () => {
      const raindexClient = extractWasmEncodedData(
        await RaindexClient.new([YAML]),
      );
      const result = extractWasmEncodedData(raindexClient.getAllTokens());
      assert.equal(result.size, 2);
      assert(result.has("token1"));
      assert(result.has("token2"));
      assert.equal(
        result.get("token1")?.address.toLowerCase(),
        "0xc2132d05d31c914a87c6611c10748aeb04b58e8f",
      );
      assert.equal(result.get("token1")?.decimals, 6);
      assert.equal(result.get("token1")?.symbol, "T1");
      assert.equal(result.get("token1")?.label, "Token 1");
      assert.equal(
        result.get("token2")?.address.toLowerCase(),
        "0x8f3cf7ad23cd3cadbd9735aff958023239c6a063",
      );
      assert.equal(result.get("token2")?.decimals, 18);
      assert.equal(result.get("token2")?.symbol, "T2");
      assert.equal(result.get("token2")?.label, "Token 2");
    });
  });
});
