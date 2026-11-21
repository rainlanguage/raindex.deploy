import { BigInt } from "@graphprotocol/graph-ts";
import { WithdrawV2 } from "../generated/Raindex/Raindex";
import { Withdrawal } from "../generated/schema";
import { eventId } from "./interfaces/event";
import { handleVaultBalanceChange, vaultEntityId } from "./vault";
import { Float, getCalculator } from "./float";
import { DecimalFloat } from "../generated/Raindex/DecimalFloat";

export function handleWithdraw(event: WithdrawV2): void {
  const calculator = getCalculator();

  let vaultBalanceChange = handleVaultBalanceChange(
    event.address,
    event.params.vaultId,
    event.params.token,
    calculator.minus(event.params.withdrawAmount),
    event.params.sender
  );
  createWithdrawalEntity(calculator, event, vaultBalanceChange.oldVaultBalance);
}

export function createWithdrawalEntity(
  calculator: DecimalFloat,
  event: WithdrawV2,
  oldVaultBalance: Float
): void {
  let withdraw = new Withdrawal(eventId(event));
  withdraw.raindex = event.address;
  withdraw.amount = calculator.minus(event.params.withdrawAmount);
  withdraw.targetAmount = event.params.targetAmount;
  withdraw.sender = event.params.sender;
  withdraw.vault = vaultEntityId(
    event.address,
    event.params.sender,
    event.params.vaultId,
    event.params.token
  );
  withdraw.transaction = event.transaction.hash;
  withdraw.oldVaultBalance = oldVaultBalance;
  withdraw.newVaultBalance = calculator.sub(
    oldVaultBalance,
    event.params.withdrawAmount
  );
  withdraw.timestamp = event.block.timestamp;
  withdraw.save();
}
