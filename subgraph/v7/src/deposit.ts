import {
  Deposit as DepositEntity,
  ERC20 as ERC20Entity,
} from "../generated/schema";
import { eventId } from "./interfaces/event";
import { handleVaultBalanceChange, vaultEntityId } from "./vault";
import { DepositV2 } from "../generated/Raindex/Raindex";
import { Float, getCalculator } from "./float";
import { getERC20Entity } from "./erc20";

export function handleDeposit(event: DepositV2): void {
  let tokenEntityId = getERC20Entity(event.params.token);
  let tokenEntity = ERC20Entity.load(tokenEntityId);

  if (!tokenEntity) {
    return;
  }

  let decimalsBigInt = tokenEntity.decimals;
  if (!decimalsBigInt) {
    return;
  }

  const decimals = decimalsBigInt.toI32();
  const calculator = getCalculator();
  let depositAmount = calculator.fromFixedDecimalLossless(
    event.params.depositAmountUint256,
    decimals
  );

  let vaultBalanceChange = handleVaultBalanceChange(
    event.address,
    event.params.vaultId,
    event.params.token,
    depositAmount,
    event.params.sender
  );

  let oldVaultBalance = vaultBalanceChange.oldVaultBalance;
  let newVaultBalance = vaultBalanceChange.newVaultBalance;

  createDepositEntity(event, oldVaultBalance, newVaultBalance, depositAmount);
}

export function createDepositEntity(
  event: DepositV2,
  oldVaultBalance: Float,
  newVaultBalance: Float,
  depositAmount: Float
): void {
  let deposit = new DepositEntity(eventId(event));
  deposit.raindex = event.address;
  deposit.amount = depositAmount;
  deposit.sender = event.params.sender;
  deposit.vault = vaultEntityId(
    event.address,
    event.params.sender,
    event.params.vaultId,
    event.params.token
  );
  deposit.transaction = event.transaction.hash;
  deposit.oldVaultBalance = oldVaultBalance;
  deposit.newVaultBalance = newVaultBalance;
  deposit.timestamp = event.block.timestamp;
  deposit.save();
}
