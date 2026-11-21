import { DecimalFloat } from "../generated/Raindex/DecimalFloat";
import { AfterClearV2, ClearV3 } from "../generated/Raindex/Raindex";
import { Clear, ClearBounty, ClearTemporaryData } from "../generated/schema";
import { Float, getCalculator } from "./float";
import { eventId } from "./interfaces/event";
import { createTradeEntity } from "./trade";
import { createTradeVaultBalanceChangeEntity } from "./tradevaultbalancechange";
import { handleVaultBalanceChange, vaultEntityId, VaultId } from "./vault";
import { log } from "@graphprotocol/graph-ts";
import {
  BigInt,
  Bytes,
  crypto,
  ethereum,
  store,
} from "@graphprotocol/graph-ts";

export function makeClearEventId(event: ethereum.Event): Bytes {
  return Bytes.fromByteArray(
    crypto.keccak256(event.address.concat(event.transaction.hash))
  );
}

export function getOrdersHash(event: ClearV3): Bytes[] {
  return [
    Bytes.fromByteArray(
      crypto.keccak256(ethereum.encode(event.parameters[1].value)!)
    ),
    Bytes.fromByteArray(
      crypto.keccak256(ethereum.encode(event.parameters[2].value)!)
    ),
  ];
}

export function makeClearBountyId(
  event: AfterClearV2,
  vaultEntityId: Bytes
): Bytes {
  return Bytes.fromByteArray(
    crypto.keccak256(vaultEntityId.concat(eventId(event)))
  );
}

export function createTrade(
  event: AfterClearV2,
  owner: Bytes,
  orderHash: Bytes,
  inputToken: Bytes,
  inputVaultId: VaultId,
  inputAmount: Float,
  outputToken: Bytes,
  outputVaultId: VaultId,
  outputAmount: Float
): void {
  const calculator = getCalculator();

  let inVaultBalance = handleVaultBalanceChange(
    event.address,
    inputVaultId,
    inputToken,
    inputAmount,
    owner
  );
  let oldInputVaultBalance = inVaultBalance.oldVaultBalance;

  let inputVaultBalanceChange = createTradeVaultBalanceChangeEntity(
    event,
    orderHash,
    vaultEntityId(event.address, owner, inputVaultId, inputToken),
    oldInputVaultBalance,
    inputAmount
  );

  let outVaultBalance = handleVaultBalanceChange(
    event.address,
    outputVaultId,
    outputToken,
    calculator.minus(outputAmount),
    owner
  );
  let oldOutputVaultBalance = outVaultBalance.oldVaultBalance;
  let outputVaultBalanceChange = createTradeVaultBalanceChangeEntity(
    event,
    orderHash,
    vaultEntityId(event.address, owner, outputVaultId, outputToken),
    oldOutputVaultBalance,
    calculator.minus(outputAmount)
  );

  createTradeEntity(
    event,
    orderHash,
    inputVaultBalanceChange,
    outputVaultBalanceChange
  );
}

export function createClearEntity(
  calculator: DecimalFloat,
  event: AfterClearV2,
  aliceBountyAmount: Float,
  bobBountyAmount: Float,
  aliceClearBounty: Float | null,
  bobClearBounty: Float | null,
  zero: Float
): void {
  let clear = new Clear(eventId(event));
  clear.raindex = event.address;
  clear.aliceInputAmount = event.params.clearStateChange.aliceInput;
  clear.aliceOutputAmount = event.params.clearStateChange.aliceOutput;
  clear.aliceBountyAmount = calculator.gt(aliceBountyAmount, zero)
    ? aliceBountyAmount
    : zero;

  clear.bobInputAmount = event.params.clearStateChange.bobInput;
  clear.bobOutputAmount = event.params.clearStateChange.bobOutput;
  clear.bobBountyAmount = calculator.gt(bobBountyAmount, zero)
    ? bobBountyAmount
    : zero;

  if (aliceClearBounty) {
    clear.aliceBountyVaultBalanceChange = aliceClearBounty;
  }
  if (bobClearBounty) {
    clear.bobBountyVaultBalanceChange = bobClearBounty;
  }

  clear.sender = event.params.sender;
  clear.transaction = event.transaction.hash;
  clear.save();
}

export function createClearBountyEntity(
  event: AfterClearV2,
  vaultEntityId: Bytes,
  oldVaultBalance: Float,
  amount: Float
): ClearBounty {
  const calculator = getCalculator();

  let clearBounty = new ClearBounty(makeClearBountyId(event, vaultEntityId));
  clearBounty.raindex = event.address;
  clearBounty.amount = amount;
  clearBounty.oldVaultBalance = oldVaultBalance;
  clearBounty.newVaultBalance = calculator.add(oldVaultBalance, amount);
  clearBounty.vault = vaultEntityId;
  clearBounty.timestamp = event.block.timestamp;
  clearBounty.transaction = event.transaction.hash;
  clearBounty.sender = event.params.sender;
  clearBounty.save();
  return clearBounty;
}

export function handleClearBounty(
  event: AfterClearV2,
  clearTemporaryData: ClearTemporaryData
): void {
  const calculator = getCalculator();

  const zero = Bytes.fromHexString(
    "0x0000000000000000000000000000000000000000000000000000000000000000"
  );

  let aliceClearBounty: Bytes | null = null;
  let bobClearBounty: Bytes | null = null;
  let aliceBountyAmount = calculator.sub(
    event.params.clearStateChange.aliceOutput,
    event.params.clearStateChange.bobInput
  );
  let bobBountyAmount = calculator.sub(
    event.params.clearStateChange.bobOutput,
    event.params.clearStateChange.aliceInput
  );

  if (calculator.gt(aliceBountyAmount, zero)) {
    const balanceChange = handleVaultBalanceChange(
      event.address,
      clearTemporaryData.aliceBounty,
      clearTemporaryData.aliceOutputToken,
      aliceBountyAmount,
      event.params.sender
    );

    aliceClearBounty = createClearBountyEntity(
      event,
      vaultEntityId(
        event.address,
        event.params.sender,
        clearTemporaryData.aliceBounty,
        clearTemporaryData.aliceOutputToken
      ),
      balanceChange.oldVaultBalance,
      aliceBountyAmount
    ).id;
  }

  if (calculator.gt(bobBountyAmount, zero)) {
    const balanceChange = handleVaultBalanceChange(
      event.address,
      clearTemporaryData.bobBounty,
      clearTemporaryData.bobOutputToken,
      bobBountyAmount,
      event.params.sender
    );

    bobClearBounty = createClearBountyEntity(
      event,
      vaultEntityId(
        event.address,
        event.params.sender,
        clearTemporaryData.bobBounty,
        clearTemporaryData.bobOutputToken
      ),
      balanceChange.oldVaultBalance,
      bobBountyAmount
    ).id;
  }

  createClearEntity(
    calculator,
    event,
    aliceBountyAmount,
    bobBountyAmount,
    aliceClearBounty,
    bobClearBounty,
    zero
  );
}

export function handleClear(event: ClearV3): void {
  let clearTemporaryData = new ClearTemporaryData(makeClearEventId(event));

  let hashes = getOrdersHash(event);
  let aliceOrderHash = hashes[0];
  let bobOrderHash = hashes[1];

  let aliceInput =
    event.params.alice.validInputs[
      event.params.clearConfig.aliceInputIOIndex.toU32()
    ];
  let aliceOutput =
    event.params.alice.validOutputs[
      event.params.clearConfig.aliceOutputIOIndex.toU32()
    ];
  let bobInput =
    event.params.bob.validInputs[
      event.params.clearConfig.bobInputIOIndex.toU32()
    ];
  let bobOutput =
    event.params.bob.validOutputs[
      event.params.clearConfig.bobOutputIOIndex.toU32()
    ];

  clearTemporaryData.aliceOrderHash = aliceOrderHash;
  clearTemporaryData.bobOrderHash = bobOrderHash;

  clearTemporaryData.aliceAddress = event.params.alice.owner;
  clearTemporaryData.bobAddress = event.params.bob.owner;

  clearTemporaryData.aliceInputVaultId = aliceInput.vaultId;
  clearTemporaryData.aliceOutputVaultId = aliceOutput.vaultId;
  clearTemporaryData.bobInputVaultId = bobInput.vaultId;
  clearTemporaryData.bobOutputVaultId = bobOutput.vaultId;

  clearTemporaryData.aliceInputToken = aliceInput.token;
  clearTemporaryData.aliceOutputToken = aliceOutput.token;
  clearTemporaryData.bobInputToken = bobInput.token;
  clearTemporaryData.bobOutputToken = bobOutput.token;

  clearTemporaryData.aliceBounty = event.params.clearConfig.aliceBountyVaultId;
  clearTemporaryData.bobBounty = event.params.clearConfig.bobBountyVaultId;

  clearTemporaryData.save();
}

export function handleAfterClear(event: AfterClearV2): void {
  let clearTemporaryData = ClearTemporaryData.load(makeClearEventId(event));
  if (clearTemporaryData) {
    // alice
    createTrade(
      event,
      clearTemporaryData.aliceAddress,
      clearTemporaryData.aliceOrderHash,
      clearTemporaryData.aliceInputToken,
      clearTemporaryData.aliceInputVaultId,
      event.params.clearStateChange.aliceInput,
      clearTemporaryData.aliceOutputToken,
      clearTemporaryData.aliceOutputVaultId,
      event.params.clearStateChange.aliceOutput
    );
    // bob
    createTrade(
      event,
      clearTemporaryData.bobAddress,
      clearTemporaryData.bobOrderHash,
      clearTemporaryData.bobInputToken,
      clearTemporaryData.bobInputVaultId,
      event.params.clearStateChange.bobInput,
      clearTemporaryData.bobOutputToken,
      clearTemporaryData.bobOutputVaultId,
      event.params.clearStateChange.bobOutput
    );

    // bounty and clear entity
    handleClearBounty(event, clearTemporaryData);

    store.remove("ClearTemporaryData", clearTemporaryData.id.toHexString());
  } else {
    log.error("ClearTemporaryData not found for event {}", [
      event.transaction.hash.toHexString(),
    ]);
  }
}
