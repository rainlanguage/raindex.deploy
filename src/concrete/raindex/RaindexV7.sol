// SPDX-License-Identifier: LicenseRef-DCL-1.0
// SPDX-FileCopyrightText: Copyright (c) 2020 Rain Open Source Software Ltd
pragma solidity =0.8.25;

import {Math} from "@openzeppelin-contracts-5.7.0/utils/math/Math.sol";
import {Multicall} from "@openzeppelin-contracts-5.7.0/utils/Multicall.sol";
import {IERC20} from "@openzeppelin-contracts-5.7.0/token/ERC20/IERC20.sol";
import {SafeERC20} from "@openzeppelin-contracts-5.7.0/token/ERC20/utils/SafeERC20.sol";
import {ReentrancyGuard} from "@openzeppelin-contracts-5.7.0/utils/ReentrancyGuard.sol";
import {IERC20Metadata} from "@openzeppelin-contracts-5.7.0/token/ERC20/extensions/IERC20Metadata.sol";

import {LibContext} from "rainlang-0.2.40/src/lib/caller/LibContext.sol";
import {LibBytecode} from "rainlang-0.2.40/src/lib/bytecode/LibBytecode.sol";
import {LibInterpreterStateDataContract} from "rainlang-0.2.40/src/lib/state/LibInterpreterStateDataContract.sol";
import {
    SourceIndexV2,
    StateNamespace,
    IInterpreterV4,
    StackItem,
    EvalV4
} from "rainlang-0.2.40/src/interface/IInterpreterV4.sol";
import {LibUint256Array} from "rain-solmem-0.1.28/src/lib/LibUint256Array.sol";
import {LibUint256Matrix} from "rain-solmem-0.1.28/src/lib/LibUint256Matrix.sol";
import {IInterpreterStoreV3} from "rainlang-0.2.40/src/interface/IInterpreterStoreV3.sol";
import {LibNamespace} from "rainlang-0.2.40/src/lib/ns/LibNamespace.sol";
import {LibMeta} from "rain-metadata-0.1.46/src/lib/LibMeta.sol";
import {IMetaV1_2} from "rain-metadata-0.1.46/src/interface/unstable/IMetaV1_2.sol";
import {LibRaindex} from "../../lib/LibRaindex.sol";
import {LibDecimalFloat} from "rain-math-float-0.3.13/src/lib/LibDecimalFloat.sol";
import {LibTOFUTokenDecimals, TOFUOutcome} from "rain-tofu-erc20-decimals-0.1.5/src/lib/LibTOFUTokenDecimals.sol";
import {ITOFUTokenDecimals} from "rain-tofu-erc20-decimals-0.1.5/src/interface/ITOFUTokenDecimals.sol";

import {
    IRaindexV7,
    OrderV4,
    OrderConfigV4,
    TakeOrderConfigV5,
    TakeOrdersConfigV6,
    ClearConfigV2,
    ClearStateChangeV2,
    SignedContextV2,

    //forge-lint: disable-next-line(unused-import)
    EvaluableV4,
    TaskV3,
    QuoteV3,
    Float,
    IOV2
} from "raindex-interface-0.1.17/src/interface/IRaindexV7.sol";
import {IRaindexV7OrderTaker} from "raindex-interface-0.1.17/src/interface/IRaindexV7OrderTaker.sol";
import {LibOrder} from "../../lib/LibOrder.sol";
import {
    CALLING_CONTEXT_COLUMNS,
    CONTEXT_CALLING_CONTEXT_COLUMN,
    CONTEXT_DEPOSIT_COLUMN,
    CONTEXT_WITHDRAW_COLUMN,
    CONTEXT_CALCULATIONS_COLUMN,
    CONTEXT_CALCULATIONS_ROWS,
    CONTEXT_VAULT_IO_BALANCE_DIFF,
    CONTEXT_VAULT_INPUTS_COLUMN,
    CONTEXT_VAULT_IO_TOKEN,
    CONTEXT_VAULT_OUTPUTS_COLUMN,
    CONTEXT_VAULT_IO_VAULT_ID
} from "../../lib/LibRaindex.sol";
import {RaindexV7FlashLender} from "../../abstract/RaindexV7FlashLender.sol";
import {LibBytes32Array} from "rain-solmem-0.1.28/src/lib/LibBytes32Array.sol";

import {LibFormatDecimalFloat} from "rain-math-float-0.3.13/src/lib/format/LibFormatDecimalFloat.sol";
import {
    NotOrderOwner,
    TokenMismatch,
    MinimumIO,
    SameOwner,
    UnsupportedCalculateOutputs,
    NegativeVaultBalance,
    NegativeVaultBalanceChange,
    NegativePull,
    NegativePush,
    NegativeBounty,
    ClearZeroAmount
} from "../../error/ErrRaindexV7.sol";
import {ORDER_LIVE, ORDER_DEAD} from "../../constant/OrderLiveness.sol";
import {
    CALCULATE_ORDER_ENTRYPOINT,
    HANDLE_IO_ENTRYPOINT,
    CALCULATE_ORDER_MIN_OUTPUTS
} from "../../constant/OrderEntrypoints.sol";
import {OrderIOCalculationV4} from "../../struct/OrderIOCalculationV4.sol";
import {TakeOrdersIO} from "../../struct/TakeOrdersIO.sol";

/// @title RaindexV7
/// See `IRaindexV7` for more documentation.
contract RaindexV7 is IRaindexV7, IMetaV1_2, ReentrancyGuard, Multicall, RaindexV7FlashLender {
    using LibUint256Array for uint256[];
    using SafeERC20 for IERC20;
    using LibOrder for OrderV4;
    using LibUint256Array for uint256;
    using LibDecimalFloat for Float;
    using LibBytes32Array for bytes32;

    /// All hashes of all active orders. There's nothing interesting in the value
    /// it's just nonzero if the order is live. The key is the hash of the order.
    /// Removing an order sets the value back to zero so it is identical to the
    /// order never existing.
    /// The order hash includes its owner so there's no need to build a multi
    /// level mapping, each order hash MUST uniquely identify the order globally.
    /// order hash => order is live
    // Solhint and slither disagree on this. Slither wins.
    //solhint-disable-next-line private-vars-leading-underscore
    mapping(bytes32 orderHash => uint256 liveness) internal sOrders;

    /// @dev Vault balances are stored in a mapping of owner => token => vault ID
    /// This gives 1:1 parity with the `IRaindexV7` interface but keeping the
    /// `sFoo` naming convention for storage variables.
    // Solhint and slither disagree on this. Slither wins.
    //solhint-disable-next-line private-vars-leading-underscore
    mapping(address owner => mapping(address token => mapping(bytes32 vaultId => Float balance))) internal
        sVaultBalances;

    /// @inheritdoc IRaindexV7
    function vaultBalance2(address owner, address token, bytes32 vaultId) external view override returns (Float) {
        return _vaultBalance(owner, token, vaultId);
    }

    /// @dev Returns the effective vault balance. For vault ID 0, returns the
    /// minimum of the owner's token balance and their approval to this contract.
    //slither-disable-next-line calls-loop
    function _vaultBalance(address owner, address token, bytes32 vaultId) internal view returns (Float) {
        if (vaultId != bytes32(0)) {
            return sVaultBalances[owner][token][vaultId];
        } else {
            uint8 decimals = _safeDecimalsReadOnly(token);
            //slither-disable-next-line unused-return
            (Float ownerTokenBalance,) =
                LibDecimalFloat.fromFixedDecimalLossyPacked(IERC20(token).balanceOf(owner), decimals);
            //slither-disable-next-line unused-return
            (Float ownerTokenApproval,) =
                LibDecimalFloat.fromFixedDecimalLossyPacked(IERC20(token).allowance(owner, address(this)), decimals);
            // Vault 0's effective balance is the live wallet (capped by allowance)
            // plus any credit accrued so far in this batch but not yet pushed out
            // (the vault-0 slot, always zero at rest). The slot is an additive
            // delta on the wallet, never a replacement, so a later order in the
            // same batch sees an earlier order's vault-0 credit.
            return ownerTokenBalance.min(ownerTokenApproval).add(sVaultBalances[owner][token][bytes32(0)]);
        }
    }

    /// @inheritdoc IRaindexV7
    function orderExists(bytes32 orderHash) external view override returns (bool) {
        return sOrders[orderHash] == ORDER_LIVE;
    }

    /// @inheritdoc IRaindexV7
    function entask3(TaskV3[] calldata post) external nonReentrant {
        LibRaindex.doPost(new bytes32[][](0), post);
    }

    /// @inheritdoc IRaindexV7
    function deposit5(address token, bytes32 vaultId, Float depositAmount, TaskV3[] calldata post)
        external
        nonReentrant
        nonZeroVaultId(msg.sender, token, vaultId)
    {
        if (!depositAmount.gt(LibDecimalFloat.FLOAT_ZERO)) {
            revert ZeroDepositAmount(msg.sender, token, vaultId);
        }

        (uint256 depositAmountUint256, uint8 decimals) = pullTokens(msg.sender, token, depositAmount);

        // It is safest with vault deposits to move tokens in to the Raindex
        // before updating internal vault balances although we have a reentrancy
        // guard in place anyway.
        emit DepositV2(msg.sender, token, vaultId, depositAmountUint256);

        (Float beforeBalance, Float afterBalance) = increaseVaultBalance(msg.sender, token, vaultId, depositAmount);

        if (post.length != 0) {
            LibRaindex.doPost(
                LibRaindex.callingContextColumn(
                    CONTEXT_DEPOSIT_COLUMN,
                    LibBytes32Array.arrayFrom(
                        bytes32(uint256(uint160(token))),
                        bytes32(vaultId),
                        Float.unwrap(beforeBalance),
                        Float.unwrap(afterBalance),
                        bytes32(uint256(decimals)),
                        bytes32(uint256(uint160(msg.sender)))
                    )
                ),
                post
            );
        }
    }

    /// @inheritdoc IRaindexV7
    function withdraw5(address token, bytes32 vaultId, Float targetAmount, TaskV3[] calldata post)
        external
        nonReentrant
        nonZeroVaultId(msg.sender, token, vaultId)
    {
        Float withdrawAmount;
        uint8 decimals;
        Float beforeBalance;
        Float afterBalance;
        {
            if (!targetAmount.gt(LibDecimalFloat.FLOAT_ZERO)) {
                revert ZeroWithdrawTargetAmount(msg.sender, token, vaultId);
            }
            Float currentVaultBalance = _vaultBalance(msg.sender, token, vaultId);
            withdrawAmount = targetAmount.min(currentVaultBalance);
            (beforeBalance, afterBalance) = decreaseVaultBalance(msg.sender, token, vaultId, withdrawAmount);

            uint256 withdrawAmountUint256;
            (withdrawAmountUint256, decimals) = pushTokens(msg.sender, token, withdrawAmount);
            emit WithdrawV2(msg.sender, token, vaultId, targetAmount, withdrawAmount, withdrawAmountUint256);
        }

        if (post.length != 0) {
            LibRaindex.doPost(
                LibRaindex.callingContextColumn(
                    CONTEXT_WITHDRAW_COLUMN,
                    LibBytes32Array.arrayFrom(
                        bytes32(uint256(uint160(token))),
                        vaultId,
                        Float.unwrap(beforeBalance),
                        Float.unwrap(afterBalance),
                        Float.unwrap(targetAmount),
                        bytes32(uint256(decimals)),
                        bytes32(uint256(uint160(msg.sender)))
                    )
                ),
                post
            );
        }
    }

    /// @inheritdoc IRaindexV7
    function addOrder5(OrderConfigV4 calldata orderConfig, TaskV3[] calldata post)
        external
        nonReentrant
        returns (bool)
    {
        // An order that lacks the calculate and handle IO entrypoints is
        // unevaluable so it MUST NOT be added.
        uint256 sourceCount =
            LibBytecode.sourceCount(LibInterpreterStateDataContract.bytecodeOf(orderConfig.evaluable.bytecode));
        if (sourceCount <= SourceIndexV2.unwrap(CALCULATE_ORDER_ENTRYPOINT)) {
            revert OrderNoSources();
        }
        if (sourceCount <= SourceIndexV2.unwrap(HANDLE_IO_ENTRYPOINT)) {
            revert OrderNoHandleIO();
        }

        if (orderConfig.validInputs.length == 0) {
            revert OrderNoInputs();
        }
        if (orderConfig.validOutputs.length == 0) {
            revert OrderNoOutputs();
        }

        // Merge our view on the sender/owner and handle IO emptiness with the
        // config and deployer's view on the `EvaluableV2` to produce the final
        // order.
        OrderV4 memory order = OrderV4(
            msg.sender, orderConfig.evaluable, orderConfig.validInputs, orderConfig.validOutputs, orderConfig.nonce
        );
        bytes32 orderHash = order.hash();

        bool stateChange = sOrders[orderHash] == ORDER_DEAD;

        // If the order is not dead we return early without state changes.
        if (stateChange) {
            // This has to come after the external call to deploy the expression
            // because the order hash is derived from the expression and DISPair
            // addresses.
            //slither-disable-next-line reentrancy-benign
            sOrders[orderHash] = ORDER_LIVE;
            emit AddOrderV3(order.owner, orderHash, order);

            // We only emit the meta event if there is meta to emit. We do require
            // that the meta self describes as a Rain meta document.
            if (orderConfig.meta.length > 0) {
                LibMeta.checkMetaUnhashedV1(orderConfig.meta);
                emit MetaV1_2(order.owner, orderHash, orderConfig.meta);
            }

            _doOrderPost(orderHash, post);
        }

        return stateChange;
    }

    /// @inheritdoc IRaindexV7
    function removeOrder4(OrderV4 calldata order, TaskV3[] calldata post)
        external
        nonReentrant
        returns (bool stateChanged)
    {
        if (msg.sender != order.owner) {
            revert NotOrderOwner(order.owner);
        }
        bytes32 orderHash = order.hash();
        if (sOrders[orderHash] == ORDER_LIVE) {
            stateChanged = true;
            sOrders[orderHash] = ORDER_DEAD;
            emit RemoveOrderV3(msg.sender, orderHash, order);

            _doOrderPost(orderHash, post);
        }
    }

    /// @dev Runs the post tasks for an order mutation (`addOrder5` / `removeOrder4`)
    /// with a context of the order hash and the caller.
    function _doOrderPost(bytes32 orderHash, TaskV3[] calldata post) internal {
        LibRaindex.doPost(
            LibRaindex.callingContextColumn(
                CONTEXT_CALLING_CONTEXT_COLUMN,
                LibBytes32Array.arrayFrom(orderHash, bytes32(uint256(uint160(msg.sender))))
            ),
            post
        );
    }

    /// @dev Reverts with `TokenSelfTrade` if the input and output tokens are
    /// the same address.
    function checkTokenSelfTrade(OrderV4 memory order, uint256 inputIOIndex, uint256 outputIOIndex) internal pure {
        if (order.validInputs[inputIOIndex].token == order.validOutputs[outputIOIndex].token) {
            revert TokenSelfTrade();
        }
    }

    /// @inheritdoc IRaindexV7
    function quote3(QuoteV3 calldata quoteConfig) external view returns (bool, Float, Float) {
        bytes32 orderHash = quoteConfig.order.hash();

        if (sOrders[orderHash] != ORDER_LIVE) {
            // false is quote3's "no live order" result, not a constant condition.
            // forge-lint: disable-next-line(boolean-cst)
            return (false, LibDecimalFloat.FLOAT_ZERO, LibDecimalFloat.FLOAT_ZERO);
        }

        checkTokenSelfTrade(quoteConfig.order, quoteConfig.inputIOIndex, quoteConfig.outputIOIndex);

        (, OrderIOCalculationV4 memory orderIOCalculation) = calculateOrderIO(
            quoteConfig.order,
            quoteConfig.inputIOIndex,
            quoteConfig.outputIOIndex,
            msg.sender,
            quoteConfig.signedContext,
            new bytes32[](0),
            false,
            false
        );
        // true is quote3's "order is live" result required by IRaindexV7.
        // forge-lint: disable-next-line(boolean-cst)
        return (true, orderIOCalculation.outputMax, orderIOCalculation.IORatio);
    }

    /// @inheritdoc IRaindexV7
    // Most of the cyclomatic complexity here is due to the error handling within
    // the loop. The actual logic is fairly linear.
    //slither-disable-next-line cyclomatic-complexity
    function takeOrders5(TakeOrdersConfigV6 calldata config)
        external
        nonReentrant
        returns (Float totalTakerInput, Float totalTakerOutput)
    {
        if (config.orders.length == 0) {
            revert NoOrders();
        }

        // Guard the max IO before dereferencing any order IO index so a zero max
        // reverts with this explicit error rather than a bounds panic when an IO
        // index is also out of range.
        if (!config.maximumIO.gt(LibDecimalFloat.FLOAT_ZERO)) {
            revert ZeroMaximumIO();
        }

        TakeOrderConfigV5 memory takeOrderConfig;
        OrderV4 memory order;
        TakeOrdersIO memory io = TakeOrdersIO({
            inputToken: config.orders[0].order.validInputs[config.orders[0].inputIOIndex].token,
            outputToken: config.orders[0].order.validOutputs[config.orders[0].outputIOIndex].token,
            remainingTakerIO: config.maximumIO
        });

        // Allocate a region of memory to hold pointers. We don't know how many
        // will run at this point, but we conservatively set aside a slot for
        // every order in case we need it, rather than attempt to dynamically
        // resize the array later. There's no guarantee that a dynamic solution
        // would even be cheaper gas-wise, and it would almost certainly be more
        // complex.
        OrderIOCalculationV4[] memory orderIOCalculationsToHandle;
        {
            uint256 length = config.orders.length;
            assembly ("memory-safe") {
                let ptr := mload(0x40)
                orderIOCalculationsToHandle := ptr
                mstore(ptr, 0)
                mstore(0x40, add(ptr, mul(add(length, 1), 0x20)))
            }
        }

        // True once any order in the batch is taken (a strictly positive fill).
        // Gates the `onTakeOrders2` callback so it only fires when the taker
        // received a non-zero input.
        bool ordersTaken;

        {
            uint256 i = 0;
            while (i < config.orders.length && io.remainingTakerIO.gt(LibDecimalFloat.FLOAT_ZERO)) {
                takeOrderConfig = config.orders[i];
                order = takeOrderConfig.order;
                // Every order needs the same input token.
                // Every order needs the same output token.
                if (
                    (order.validInputs[takeOrderConfig.inputIOIndex].token != io.inputToken)
                        || (order.validOutputs[takeOrderConfig.outputIOIndex].token != io.outputToken)
                ) {
                    revert TokenMismatch();
                }

                checkTokenSelfTrade(order, takeOrderConfig.inputIOIndex, takeOrderConfig.outputIOIndex);

                bytes32 orderHash = order.hash();
                if (sOrders[orderHash] == ORDER_DEAD) {
                    emit OrderNotFound(msg.sender, order.owner, orderHash);
                } else {
                    // No state overlay: calculate reads only committed store
                    // state. Each taken order commits its calculate and handle-IO
                    // writes via `handleIO` below, before the next order
                    // calculates, so a later same-owner order observes earlier
                    // TAKEN orders' writes through the store. An untaken order
                    // runs no `handleIO`, so it makes no state change at all.
                    (bool calculated, OrderIOCalculationV4 memory orderIOCalculation) = calculateOrderIO(
                        order,
                        takeOrderConfig.inputIOIndex,
                        takeOrderConfig.outputIOIndex,
                        msg.sender,
                        takeOrderConfig.signedContext,
                        new bytes32[](0),
                        true,
                        false
                    );

                    // Skip orders that are too expensive rather than revert as we have
                    // no way of knowing if a specific order becomes too expensive
                    // between submitting to mempool and execution, but other orders may
                    // be valid so we want to take advantage of those if possible.
                    //
                    // An order is taken only when its calculation yields a
                    // strictly positive output at a non-negative ratio within
                    // the taker's limit. A zero ratio gives its output for
                    // nothing. The explicit `taken` flag is the order's outcome
                    // for the batch.
                    bool taken;
                    if (!calculated) {
                        emit OrderCalculateReverted(msg.sender, order.owner, orderHash);
                    } else if (orderIOCalculation.IORatio.gt(config.maximumIORatio)) {
                        emit OrderExceedsMaxRatio(msg.sender, order.owner, orderHash);
                    } else if (
                        !orderIOCalculation.outputMax.gt(LibDecimalFloat.FLOAT_ZERO)
                            || orderIOCalculation.IORatio.lt(LibDecimalFloat.FLOAT_ZERO)
                    ) {
                        emit OrderZeroAmount(msg.sender, order.owner, orderHash);
                    } else {
                        taken = true;
                        Float takerInput;
                        Float takerOutput;
                        if (config.IOIsInput) {
                            // Taker is just "market buying" the order output max.
                            // Can't exceed the remaining taker input.
                            takerInput = orderIOCalculation.outputMax.min(io.remainingTakerIO);

                            // Slither false positive because it sees the div on
                            // the else branch and triggers a
                            // divide-before-multiply warning. In this case the
                            // IOIsInput on the config is always the same for
                            // every order in the loop so we always take the same
                            // branch, whether this or the else.
                            //slither-disable-next-line divide-before-multiply
                            takerOutput = orderIOCalculation.IORatio.mul(takerInput);

                            io.remainingTakerIO = io.remainingTakerIO.sub(takerInput);
                        } else {
                            // Taker is "market selling" up to the order output max.
                            Float orderMaxInput = orderIOCalculation.IORatio.mul(orderIOCalculation.outputMax);
                            takerOutput = orderMaxInput.min(io.remainingTakerIO);
                            // This rounds down which favours the order/dex.
                            takerInput = orderIOCalculation.IORatio.isZero()
                                ? orderIOCalculation.outputMax
                                : takerOutput.div(orderIOCalculation.IORatio);

                            io.remainingTakerIO = io.remainingTakerIO.sub(takerOutput);
                        }

                        totalTakerOutput = totalTakerOutput.add(takerOutput);
                        totalTakerInput = totalTakerInput.add(takerInput);

                        // Pull the order's output into the orderbook now; the
                        // vault-0 input push is settled after the taker's payment
                        // is pulled below, so the orderbook holds the input token
                        // before it is pushed to the owner.
                        recordVaultOutput(takerInput, orderIOCalculation);
                        recordVaultInput(takerOutput, orderIOCalculation);
                        emit TakeOrderV4(msg.sender, takeOrderConfig, takerInput, takerOutput);

                        // Add the pointer to the order IO calculation to the array
                        // of order IO calculations to handle. This is
                        // unconditional because conditional behaviour is checked
                        // in `handleIO` and we don't want to duplicate that.
                        assembly ("memory-safe") {
                            // Inc the length by 1.
                            let newLength := add(mload(orderIOCalculationsToHandle), 1)
                            mstore(orderIOCalculationsToHandle, newLength)
                            // Store the pointer to the order IO calculation.
                            mstore(add(orderIOCalculationsToHandle, mul(newLength, 0x20)), orderIOCalculation)
                        }

                        // Run handle IO for this order before the next order
                        // calculates, committing its calculate and handle-IO store
                        // writes so a later order's calculate reads them. Handle IO
                        // sees this order's vault-0 input via the accrued slot; the
                        // push itself is still deferred to after the taker pull.
                        handleIO(orderIOCalculation);
                    }

                    ordersTaken = ordersTaken || taken;
                }

                unchecked {
                    i++;
                }
            }
        }

        {
            Float actualRelevantTakerIO = config.IOIsInput ? totalTakerInput : totalTakerOutput;
            if (actualRelevantTakerIO.lt(config.minimumIO)) {
                revert MinimumIO(config.minimumIO, actualRelevantTakerIO);
            }
        }

        // We send the tokens to `msg.sender` first adopting a similar pattern to
        // Uniswap flash swaps. We call the caller before attempting to pull
        // tokens from them in order to facilitate better integrations with
        // external liquidity sources. This could be done by the caller using
        // flash loans but this callback:
        // - may be simpler for the caller to implement
        // - allows the caller to call `takeOrders` _before_ placing external
        //   trades, which is important if the order logic itself is dependent on
        //   external data (e.g. prices) that could be modified by the caller's
        //   trades.

        pushTokens(msg.sender, io.outputToken, totalTakerInput);

        // Integrator note on the state observed inside `onTakeOrders2`: the
        // orders are fully recorded and handled and the taker has received
        // `io.outputToken`, but their `io.inputToken` payment has not been pulled
        // yet and the vault-0 input settlements below have not run. For the
        // duration of the callback the contract's actual token balance is below
        // its internal vault accounting: `vaultBalance2` already reflects the
        // post-trade balances, and `maxFlashLoan` returns a correspondingly
        // reduced amount until settlement completes. Re-entering raindex from the
        // callback is blocked by `nonReentrant`.
        if (config.data.length > 0 && ordersTaken) {
            IRaindexV7OrderTaker(msg.sender)
                .onTakeOrders2(io.outputToken, io.inputToken, totalTakerInput, totalTakerOutput, config.data);
        }

        pullTokens(msg.sender, io.inputToken, totalTakerOutput);

        // The taker's payment has now been pulled in, so the orderbook holds the
        // input token. Settle the vault-0 input pushes accrued during the loop.
        // Every order shares `io.inputToken` and the sum of these pushes equals
        // `totalTakerOutput`, so the orderbook always holds enough. Handle IO
        // already ran per order in the loop, against the accrued vault-0 balances.
        unchecked {
            for (uint256 i = 0; i < orderIOCalculationsToHandle.length; i++) {
                pushVaultZeroInput(orderIOCalculationsToHandle[i]);
            }
        }
    }

    /// @inheritdoc IRaindexV7
    function clear4(
        OrderV4 memory aliceOrder,
        OrderV4 memory bobOrder,
        ClearConfigV2 calldata clearConfig,
        SignedContextV2[] memory aliceSignedContext,
        SignedContextV2[] memory bobSignedContext
    ) external nonReentrant {
        {
            if (aliceOrder.owner == bobOrder.owner) {
                revert SameOwner();
            }
            if (
                (aliceOrder.validOutputs[clearConfig.aliceOutputIOIndex].token
                            != bobOrder.validInputs[clearConfig.bobInputIOIndex].token)
                    || (bobOrder.validOutputs[clearConfig.bobOutputIOIndex].token
                            != aliceOrder.validInputs[clearConfig.aliceInputIOIndex].token)
            ) {
                revert TokenMismatch();
            }

            if (
                aliceOrder.validInputs[clearConfig.aliceInputIOIndex].token
                    == aliceOrder.validOutputs[clearConfig.aliceOutputIOIndex].token
            ) {
                revert TokenSelfTrade();
            }

            // If either order is dead the clear is a no-op other than emitting
            // `OrderNotFound`. Returning rather than erroring makes it easier to
            // bulk clear using `Multicall`.
            bytes32 aliceOrderHash = aliceOrder.hash();
            if (sOrders[aliceOrderHash] == ORDER_DEAD) {
                emit OrderNotFound(msg.sender, aliceOrder.owner, aliceOrderHash);
                return;
            }
            bytes32 bobOrderHash = bobOrder.hash();
            if (sOrders[bobOrderHash] == ORDER_DEAD) {
                emit OrderNotFound(msg.sender, bobOrder.owner, bobOrderHash);
                return;
            }

            // Emit the Clear event before `eval2`.
            emit ClearV3(msg.sender, aliceOrder, bobOrder, clearConfig);
        }
        (, OrderIOCalculationV4 memory aliceOrderIOCalculation) = calculateOrderIO(
            aliceOrder,
            clearConfig.aliceInputIOIndex,
            clearConfig.aliceOutputIOIndex,
            bobOrder.owner,
            aliceSignedContext,
            new bytes32[](0),
            false,
            true
        );
        (, OrderIOCalculationV4 memory bobOrderIOCalculation) = calculateOrderIO(
            bobOrder,
            clearConfig.bobInputIOIndex,
            clearConfig.bobOutputIOIndex,
            aliceOrder.owner,
            bobSignedContext,
            new bytes32[](0),
            false,
            true
        );

        // Each side must be one `takeOrders5` would take. Given this, each
        // computed output is positive or the Float math reverts.
        if (
            !aliceOrderIOCalculation.outputMax.gt(LibDecimalFloat.FLOAT_ZERO)
                || !bobOrderIOCalculation.outputMax.gt(LibDecimalFloat.FLOAT_ZERO)
                || aliceOrderIOCalculation.IORatio.lt(LibDecimalFloat.FLOAT_ZERO)
                || bobOrderIOCalculation.IORatio.lt(LibDecimalFloat.FLOAT_ZERO)
        ) {
            revert ClearZeroAmount();
        }

        ClearStateChangeV2 memory clearStateChange =
            calculateClearStateChange(aliceOrderIOCalculation, bobOrderIOCalculation);

        Float aliceBounty = clearStateChange.aliceOutput.sub(clearStateChange.bobInput);
        Float bobBounty = clearStateChange.bobOutput.sub(clearStateChange.aliceInput);

        // A negative bounty means there is a spread between the orders. This is a
        // critical error because it means the DEX could be exploited if allowed.
        // Checked before any vault settlement so a spread always reverts with this
        // explicit error.
        if (aliceBounty.lt(LibDecimalFloat.FLOAT_ZERO) || bobBounty.lt(LibDecimalFloat.FLOAT_ZERO)) {
            revert NegativeBounty();
        }

        // Pull both orders' outputs into the orderbook before pushing either
        // order's input out. Alice's input token is Bob's output token and vice
        // versa, so both pulls must precede both pushes for the orderbook to hold
        // each token before it is pushed.
        recordVaultOutput(clearStateChange.aliceOutput, aliceOrderIOCalculation);
        recordVaultOutput(clearStateChange.bobOutput, bobOrderIOCalculation);
        recordVaultInput(clearStateChange.aliceInput, aliceOrderIOCalculation);
        recordVaultInput(clearStateChange.bobInput, bobOrderIOCalculation);
        pushVaultZeroInput(aliceOrderIOCalculation);
        pushVaultZeroInput(bobOrderIOCalculation);

        {
            increaseVaultBalance(
                msg.sender,
                aliceOrder.validOutputs[clearConfig.aliceOutputIOIndex].token,
                clearConfig.aliceBountyVaultId,
                aliceBounty
            );
            increaseVaultBalance(
                msg.sender,
                bobOrder.validOutputs[clearConfig.bobOutputIOIndex].token,
                clearConfig.bobBountyVaultId,
                bobBounty
            );
        }

        emit AfterClearV2(msg.sender, clearStateChange);

        handleIO(aliceOrderIOCalculation);
        handleIO(bobOrderIOCalculation);
    }

    /// Main entrypoint into an order calculates the amount and IO ratio. Both
    /// are always treated as Float values and then rescaled according to the
    /// order's definition of each token's actual fixed point decimals.
    /// @param order The order to evaluate.
    /// @param inputIOIndex The index of the input token being calculated for.
    /// @param outputIOIndex The index of the output token being calculated for.
    /// @param counterparty The counterparty of the order as it is currently
    /// being cleared against.
    /// @param signedContext Any signed context provided by the clearer/taker
    /// that the order may need for its calculations.
    /// @param stateOverlay Key/value pairs seeded into the interpreter's
    /// in-memory store before the calculate eval, making earlier same-owner,
    /// same-batch calculate writes visible to this evaluation. Empty for
    /// single-order callers (`quote`, `clear4`).
    /// @param skipFailure If true, a reverting calculate eval or a stack shorter
    /// than `CALCULATE_ORDER_MIN_OUTPUTS` returns false instead of reverting.
    /// @param clearing Whether the order column includes the clearer row; true
    /// only for `clear4`.
    /// @return False if `skipFailure` caught a failure, in which case the
    /// calculation is zeroed and MUST NOT be used.
    /// @return The order IO calculation.
    function calculateOrderIO(
        OrderV4 memory order,
        uint256 inputIOIndex,
        uint256 outputIOIndex,
        address counterparty,
        SignedContextV2[] memory signedContext,
        bytes32[] memory stateOverlay,
        bool skipFailure,
        bool clearing
    ) internal view returns (bool, OrderIOCalculationV4 memory) {
        unchecked {
            bytes32[][] memory context;
            {
                bytes32[][] memory callingContext = new bytes32[][](CALLING_CONTEXT_COLUMNS);
                callingContext[CONTEXT_CALLING_CONTEXT_COLUMN - 1] = clearing
                    ? LibBytes32Array.arrayFrom(
                        order.hash(),
                        bytes32(uint256(uint160(order.owner))),
                        bytes32(uint256(uint160(counterparty))),
                        bytes32(uint256(uint160(msg.sender)))
                    )
                    : LibBytes32Array.arrayFrom(
                        order.hash(), bytes32(uint256(uint160(order.owner))), bytes32(uint256(uint160(counterparty)))
                    );

                // The calculations column holds the calculated max output and IO
                // ratio, which only become known after the calculate eval below.
                // Seed it with a zero-filled array of the right length so the
                // `calculated-max-output` and `calculated-io-ratio` words read 0
                // during calculate (per their NatSpec) instead of reverting on an
                // out-of-bounds read. The real values overwrite this column after
                // eval, before handle IO runs.
                callingContext[CONTEXT_CALCULATIONS_COLUMN - 1] = new bytes32[](CONTEXT_CALCULATIONS_ROWS);

                {
                    uint8 inputDecimals = _safeDecimalsReadOnly(order.validInputs[inputIOIndex].token);

                    Float inputTokenVaultBalance = _vaultBalance(
                        order.owner, order.validInputs[inputIOIndex].token, order.validInputs[inputIOIndex].vaultId
                    );
                    callingContext[CONTEXT_VAULT_INPUTS_COLUMN - 1] = LibBytes32Array.arrayFrom(
                        bytes32(uint256(uint160(order.validInputs[inputIOIndex].token))),
                        bytes32(uint256(inputDecimals)),
                        order.validInputs[inputIOIndex].vaultId,
                        Float.unwrap(inputTokenVaultBalance),
                        // Don't know the balance diff yet!
                        0
                    );
                }

                {
                    uint8 outputDecimals = _safeDecimalsReadOnly(order.validOutputs[outputIOIndex].token);

                    Float outputTokenVaultBalance = _vaultBalance(
                        order.owner, order.validOutputs[outputIOIndex].token, order.validOutputs[outputIOIndex].vaultId
                    );
                    callingContext[CONTEXT_VAULT_OUTPUTS_COLUMN - 1] = LibBytes32Array.arrayFrom(
                        bytes32(uint256(uint160(order.validOutputs[outputIOIndex].token))),
                        bytes32(uint256(outputDecimals)),
                        order.validOutputs[outputIOIndex].vaultId,
                        Float.unwrap(outputTokenVaultBalance),
                        // Don't know the balance diff yet!
                        0
                    );
                }

                context = LibContext.buildV2(callingContext, signedContext);
            }

            // The state changes produced here are handled in recordVaultOutput
            // and recordVaultInput so that local storage writes happen before
            // writes on the interpreter.
            StateNamespace namespace = StateNamespace.wrap(uint256(uint160(order.owner)));
            StackItem[] memory calculateOrderStack;
            bytes32[] memory calculateOrderKVs;
            // Slither false positive. External calls within loops are fine if
            // the caller controls which orders are eval'd as they can drop
            // failing calls and resubmit a new transaction.
            // https://github.com/crytic/slither/issues/880
            //slither-disable-next-line calls-loop
            try order.evaluable.interpreter
                .eval4(
                    EvalV4({
                        store: order.evaluable.store,
                        namespace: LibNamespace.qualifyNamespace(namespace, address(this)),
                        bytecode: order.evaluable.bytecode,
                        sourceIndex: CALCULATE_ORDER_ENTRYPOINT,
                        context: context,
                        inputs: new StackItem[](0),
                        stateOverlay: stateOverlay
                    })
                ) returns (
                StackItem[] memory stack, bytes32[] memory kvs
            ) {
                calculateOrderStack = stack;
                calculateOrderKVs = kvs;
            } catch {
                if (!skipFailure) {
                    assembly ("memory-safe") {
                        let ptr := mload(0x40)
                        returndatacopy(ptr, 0, returndatasize())
                        revert(ptr, returndatasize())
                    }
                }
            }

            // This is a much clearer error message and overall is more efficient
            // than solidity generic index out of bounds errors.
            bool calculated = calculateOrderStack.length >= CALCULATE_ORDER_MIN_OUTPUTS;
            if (!calculated) {
                if (skipFailure) {
                    OrderIOCalculationV4 memory failed;
                    return (calculated, failed);
                }
                revert UnsupportedCalculateOutputs(calculateOrderStack.length);
            }

            Float orderIORatio;
            Float orderOutputMax;
            assembly ("memory-safe") {
                orderIORatio := mload(add(calculateOrderStack, 0x20))
                orderOutputMax := mload(add(calculateOrderStack, 0x40))
            }

            // The order owner can't send more than the smaller of their vault
            // balance or their per-order limit. The output vault balance is the
            // balance field (index 3) of the output vault column already built
            // into the calculate context above, so it is reused rather than read
            // from storage a second time.
            orderOutputMax = orderOutputMax.min(Float.wrap(context[CONTEXT_VAULT_OUTPUTS_COLUMN][3]));

            // Populate the context with the output max rescaled and vault capped.
            context[CONTEXT_CALCULATIONS_COLUMN] =
                LibBytes32Array.arrayFrom(Float.unwrap(orderOutputMax), Float.unwrap(orderIORatio));

            return (
                calculated,
                OrderIOCalculationV4({
                    order: order,
                    outputIOIndex: outputIOIndex,
                    outputMax: orderOutputMax,
                    IORatio: orderIORatio,
                    context: context,
                    namespace: namespace,
                    kvs: calculateOrderKVs
                })
            );
        }
    }

    /// @dev Increases a vault balance by `amount`. For vault ID 0, pushes tokens
    /// directly to the owner instead. Returns (oldBalance, newBalance).
    function increaseVaultBalance(address owner, address token, bytes32 vaultId, Float amount)
        internal
        returns (Float, Float)
    {
        if (amount.lt(LibDecimalFloat.FLOAT_ZERO)) {
            revert NegativeVaultBalanceChange(amount);
        }

        // Vault ID 0 is special, it directly transfers tokens to the owner
        // rather than allocating an internal balance in a vault.
        //slither-disable-next-line incorrect-equality
        if (vaultId == bytes32(0)) {
            pushTokens(owner, token, amount);
            // The internal balance of vault 0 is always 0 as every transfer in
            // is effectively a compound matching withdrawal.
            return (LibDecimalFloat.FLOAT_ZERO, LibDecimalFloat.FLOAT_ZERO);
        } else {
            Float oldBalance = sVaultBalances[owner][token][vaultId];
            Float newBalance = oldBalance.add(amount);

            // This should never be possible as amount is positive and floats are
            // effectively impossible to overflow, but we check it anyway to be safe.
            if (newBalance.lt(LibDecimalFloat.FLOAT_ZERO)) {
                revert NegativeVaultBalance(newBalance);
            }
            sVaultBalances[owner][token][vaultId] = newBalance;

            return (oldBalance, newBalance);
        }
    }

    /// @dev Decreases a vault balance by `amount`. For vault ID 0, pulls tokens
    /// directly from the owner instead. Returns (oldBalance, newBalance).
    function decreaseVaultBalance(address owner, address token, bytes32 vaultId, Float amount)
        internal
        returns (Float, Float)
    {
        if (amount.lt(LibDecimalFloat.FLOAT_ZERO)) {
            revert NegativeVaultBalanceChange(amount);
        }

        // Vault ID 0 is special, it directly pulls tokens from the owner when
        // their balance on DEX is reduced.
        //slither-disable-next-line incorrect-equality
        if (vaultId == bytes32(0)) {
            pullTokens(owner, token, amount);
            // The internal balance of vault 0 is always 0 as every balance
            // decrease is effectively a compound matching deposit.
            return (LibDecimalFloat.FLOAT_ZERO, LibDecimalFloat.FLOAT_ZERO);
        } else {
            Float oldBalance = sVaultBalances[owner][token][vaultId];
            Float newBalance = oldBalance.sub(amount);

            // This can definitely happen, so needs to be guarded against.
            // There's no specific check anywhere else that vault balances don't go
            // negative, so this function should be used everywhere for safety.
            if (newBalance.lt(LibDecimalFloat.FLOAT_ZERO)) {
                revert NegativeVaultBalance(newBalance);
            }
            sVaultBalances[owner][token][vaultId] = newBalance;

            return (oldBalance, newBalance);
        }
    }

    /// @dev Records the OUTPUT (decrease) leg of an order's IO: writes the
    /// output balance diff into the context matrix and decreases the order
    /// owner's output vault balance. For `vaultId == 0` this pulls tokens
    /// directly INTO the orderbook. Always run before the matching
    /// `recordVaultInput` so the orderbook holds a token before it is pushed.
    /// @param output The Float output amount to debit from the order owner's
    /// output vault.
    /// @param orderIOCalculation The order IO calculation produced by
    /// `calculateOrderIO`, containing the order, context matrix and namespace.
    function recordVaultOutput(Float output, OrderIOCalculationV4 memory orderIOCalculation) internal {
        orderIOCalculation.context[CONTEXT_VAULT_OUTPUTS_COLUMN][CONTEXT_VAULT_IO_BALANCE_DIFF] = Float.unwrap(output);
        decreaseVaultBalance(
            orderIOCalculation.order.owner,
            address(uint160(uint256(orderIOCalculation.context[CONTEXT_VAULT_OUTPUTS_COLUMN][CONTEXT_VAULT_IO_TOKEN]))),
            orderIOCalculation.context[CONTEXT_VAULT_OUTPUTS_COLUMN][CONTEXT_VAULT_IO_VAULT_ID],
            output
        );
    }

    /// @dev Records the INPUT (increase) leg of an order's IO: writes the input
    /// balance diff into the context matrix, credits the order owner's input
    /// vault balance, and emits the now fully populated context. A `vaultId == 0`
    /// input has no internal balance and is ultimately a push of tokens out of
    /// the orderbook; the credit is accrued in the always-zero vault-0 slot as a
    /// transient in-batch balance so a later order's calculate reads it, and the
    /// push itself is settled by `pushVaultZeroInput` once the orderbook holds
    /// the token.
    /// @param input The Float input amount to credit to the order owner's input
    /// vault.
    /// @param orderIOCalculation The order IO calculation produced by
    /// `calculateOrderIO`, containing the order, context matrix and namespace.
    function recordVaultInput(Float input, OrderIOCalculationV4 memory orderIOCalculation) internal {
        orderIOCalculation.context[CONTEXT_VAULT_INPUTS_COLUMN][CONTEXT_VAULT_IO_BALANCE_DIFF] = Float.unwrap(input);
        address owner = orderIOCalculation.order.owner;
        address token =
            address(uint160(uint256(orderIOCalculation.context[CONTEXT_VAULT_INPUTS_COLUMN][CONTEXT_VAULT_IO_TOKEN])));
        bytes32 inputVaultId = orderIOCalculation.context[CONTEXT_VAULT_INPUTS_COLUMN][CONTEXT_VAULT_IO_VAULT_ID];
        //slither-disable-next-line incorrect-equality
        if (inputVaultId == bytes32(0)) {
            // Accrue into the vault-0 slot rather than pushing now. `_vaultBalance`
            // reads this slot on top of the wallet, so the credit is visible to
            // later orders in the same batch; `pushVaultZeroInput` settles and
            // zeroes it after the orderbook holds the token.
            if (input.lt(LibDecimalFloat.FLOAT_ZERO)) {
                revert NegativeVaultBalanceChange(input);
            }
            sVaultBalances[owner][token][inputVaultId] = sVaultBalances[owner][token][inputVaultId].add(input);
        } else {
            increaseVaultBalance(owner, token, inputVaultId, input);
        }

        // Emit the context only once in its fully populated form rather than two
        // nearly identical emissions of a partial and full context.
        emit ContextV2(msg.sender, orderIOCalculation.context);
    }

    /// @dev Settles the `vaultId == 0` input accrued by `recordVaultInput` by
    /// pushing it out of the orderbook to the order owner and zeroing the
    /// transient vault-0 slot. No-op for internal input vaults. Same-owner orders
    /// share the slot, so the net credit is pushed once (the first flush) and
    /// later flushes for the same owner/token are no-ops. Must run only after the
    /// orderbook holds the input token, i.e. after the matching pulls.
    /// @param orderIOCalculation The order IO calculation whose input leg was
    /// recorded by `recordVaultInput`.
    function pushVaultZeroInput(OrderIOCalculationV4 memory orderIOCalculation) internal {
        bytes32 inputVaultId = orderIOCalculation.context[CONTEXT_VAULT_INPUTS_COLUMN][CONTEXT_VAULT_IO_VAULT_ID];
        //slither-disable-next-line incorrect-equality
        if (inputVaultId == bytes32(0)) {
            address owner = orderIOCalculation.order.owner;
            address token = address(
                uint160(uint256(orderIOCalculation.context[CONTEXT_VAULT_INPUTS_COLUMN][CONTEXT_VAULT_IO_TOKEN]))
            );
            Float accrued = sVaultBalances[owner][token][inputVaultId];
            //slither-disable-next-line incorrect-equality
            if (!accrued.isZero()) {
                sVaultBalances[owner][token][inputVaultId] = LibDecimalFloat.FLOAT_ZERO;
                pushTokens(owner, token, accrued);
            }
        }
    }

    /// @dev Persists interpreter state writes then evaluates the handle IO
    /// entrypoint for an order, if it has one.
    function handleIO(OrderIOCalculationV4 memory orderIOCalculation) internal {
        // Apply state changes to the interpreter store after the vault balances
        // are updated, but before we call handle IO. We want handle IO to see
        // a consistent view on sets from calculate IO.
        if (orderIOCalculation.kvs.length > 0) {
            // Slither false positive. External calls within loops are fine if
            // the caller controls which orders are eval'd as they can drop
            // failing calls and resubmit a new transaction.
            // https://github.com/crytic/slither/issues/880
            //slither-disable-next-line calls-loop
            orderIOCalculation.order.evaluable.store.set(orderIOCalculation.namespace, orderIOCalculation.kvs);
        }

        // The handle IO eval is run under the same namespace as the
        // calculate order entrypoint.
        // Slither false positive. External calls within loops are fine if
        // the caller controls which orders are eval'd as they can drop
        // failing calls and resubmit a new transaction.
        // https://github.com/crytic/slither/issues/880
        //slither-disable-next-line calls-loop,unused-return
        (, bytes32[] memory handleIOKVs) = orderIOCalculation.order.evaluable.interpreter
            .eval4(
                EvalV4({
                    store: orderIOCalculation.order.evaluable.store,
                    namespace: LibNamespace.qualifyNamespace(orderIOCalculation.namespace, address(this)),
                    bytecode: orderIOCalculation.order.evaluable.bytecode,
                    sourceIndex: HANDLE_IO_ENTRYPOINT,
                    context: orderIOCalculation.context,
                    inputs: new StackItem[](0),
                    stateOverlay: new bytes32[](0)
                })
            );
        // Apply state changes to the interpreter store from the handle IO
        // entrypoint.
        if (handleIOKVs.length > 0) {
            // Slither false positive. External calls within loops are fine
            // if the caller controls which orders are eval'd as they can
            // drop failing calls and resubmit a new transaction.
            // https://github.com/crytic/slither/issues/880
            //slither-disable-next-line calls-loop
            orderIOCalculation.order.evaluable.store.set(orderIOCalculation.namespace, handleIOKVs);
        }
    }

    /// Calculates the clear state change given both order calculations for order
    /// Alice and Bob. Each order's initial output equals its own `outputMax`; its
    /// input is that output multiplied by its IO ratio. If the resulting input
    /// exceeds the counterparty's `outputMax`, the input is capped at the
    /// counterparty's `outputMax` and the output is back-calculated as input
    /// divided by IO ratio.
    /// @param aliceOrderIOCalculation Order calculation for Alice.
    /// @param bobOrderIOCalculation Order calculation for Bob.
    /// @return clearStateChange The clear state change with absolute inputs and
    /// outputs for Alice and Bob.
    function calculateClearStateChange(
        OrderIOCalculationV4 memory aliceOrderIOCalculation,
        OrderIOCalculationV4 memory bobOrderIOCalculation
    ) internal pure returns (ClearStateChangeV2 memory clearStateChange) {
        // Calculate the clear state change for Alice.
        (clearStateChange.aliceInput, clearStateChange.aliceOutput) =
            calculateClearStateAlice(aliceOrderIOCalculation, bobOrderIOCalculation);

        // Flip alice and bob to calculate bob's output.
        (clearStateChange.bobInput, clearStateChange.bobOutput) =
            calculateClearStateAlice(bobOrderIOCalculation, aliceOrderIOCalculation);
    }

    /// @dev Calculates Alice's input and output given both order calculations.
    /// Alice's output is capped by Bob's max output, and her input is derived
    /// from her IO ratio.
    function calculateClearStateAlice(
        OrderIOCalculationV4 memory aliceOrderIOCalculation,
        OrderIOCalculationV4 memory bobOrderIOCalculation
    ) internal pure returns (Float aliceInput, Float aliceOutput) {
        // Alice's input is her output * her IO ratio.
        aliceInput = aliceOrderIOCalculation.outputMax.mul(aliceOrderIOCalculation.IORatio);

        aliceOutput = aliceOrderIOCalculation.outputMax;

        // If Alice's input is greater than Bob's max output, Alice's input is
        // capped at Bob's max output.
        if (aliceInput.gt(bobOrderIOCalculation.outputMax)) {
            aliceInput = bobOrderIOCalculation.outputMax;

            // Alice's output is capped at her input / her IO ratio.
            aliceOutput = aliceInput.div(aliceOrderIOCalculation.IORatio);
        }
    }

    /// @dev Reads the token decimals via TOFU (state-writing variant) and reverts
    /// with `TokenDecimalsReadFailure` if the read outcome is not consistent or
    /// initial. Returns the decimals.
    function _safeDecimals(address token) private returns (uint8) {
        (TOFUOutcome tofuOutcome, uint8 decimals) = LibTOFUTokenDecimals.decimalsForToken(token);
        if (tofuOutcome != TOFUOutcome.Consistent && tofuOutcome != TOFUOutcome.Initial) {
            revert ITOFUTokenDecimals.TokenDecimalsReadFailure(token, tofuOutcome);
        }
        return decimals;
    }

    /// @dev Reads the token decimals via TOFU (read-only variant) and reverts with
    /// `TokenDecimalsReadFailure` if the read outcome is not consistent or initial.
    /// Returns the decimals.
    function _safeDecimalsReadOnly(address token) private view returns (uint8) {
        (TOFUOutcome tofuOutcome, uint8 decimals) = LibTOFUTokenDecimals.decimalsForTokenReadOnly(token);
        if (tofuOutcome != TOFUOutcome.Consistent && tofuOutcome != TOFUOutcome.Initial) {
            revert ITOFUTokenDecimals.TokenDecimalsReadFailure(token, tofuOutcome);
        }
        return decimals;
    }

    /// @dev Pulls `amount` of `token` from `account` via `safeTransferFrom`.
    /// Returns the fixed-decimal amount transferred and the token decimals.
    ///
    /// A lossy float->fixed-decimal conversion is rounded UP here so the contract
    /// never pulls in less than the vault accounting credits. `pushTokens` rounds
    /// DOWN for the same protocol-favoring reason. Together this keeps the
    /// orderbook solvent — its real token balance is always >= the sum of the
    /// vault balances — but each lossy conversion leaves a sub-token-unit residue
    /// in the contract that is not attributed to any vault and has no recovery
    /// path. Such dust is under one base unit per conversion and can never cause
    /// insolvency.
    function pullTokens(address account, address token, Float amount) internal returns (uint256, uint8) {
        uint8 decimals = _safeDecimals(token);
        if (amount.lt(LibDecimalFloat.FLOAT_ZERO)) {
            revert NegativePull();
        }

        (uint256 amount18, bool lossless) = LibDecimalFloat.toFixedDecimalLossy(amount, decimals);
        // Round truncation up when pulling.
        if (!lossless) {
            // This needs to be checked math as an overflow would cause tokens
            // to silently not be pulled (wraps to 0).
            ++amount18;
        }
        if (amount18 > 0) {
            IERC20(token).safeTransferFrom(account, address(this), amount18);
        }
        return (amount18, decimals);
    }

    /// @dev Pushes `amountFloat` of `token` to `account` via `safeTransfer`.
    /// Returns the fixed-decimal amount transferred and the token decimals.
    ///
    /// A lossy float->fixed-decimal conversion is truncated (rounded DOWN) here
    /// so the contract never sends more than the vault accounting debits. See
    /// `pullTokens` for the full rounding policy and its solvency / residual-dust
    /// tradeoff.
    function pushTokens(address account, address token, Float amountFloat) internal returns (uint256, uint8) {
        uint8 decimals = _safeDecimals(token);

        if (amountFloat.lt(LibDecimalFloat.FLOAT_ZERO)) {
            revert NegativePush();
        }

        //slither-disable-next-line unused-return
        (uint256 amount,) = LibDecimalFloat.toFixedDecimalLossy(amountFloat, decimals);
        if (amount > 0) {
            IERC20(token).safeTransfer(account, amount);
        }

        return (amount, decimals);
    }

    /// @dev Reverts with `ZeroVaultId` if `vaultId` is zero.
    function _nonZeroVaultId(address vaultOwner, address token, bytes32 vaultId) internal pure {
        if (vaultId == bytes32(0)) {
            revert ZeroVaultId(vaultOwner, token);
        }
    }

    modifier nonZeroVaultId(address vaultOwner, address token, bytes32 vaultId) {
        _nonZeroVaultId(vaultOwner, token, vaultId);
        _;
    }
}
