// SPDX-License-Identifier: LicenseRef-DCL-1.0
// SPDX-FileCopyrightText: Copyright (c) 2020 Rain Open Source Software Ltd
pragma solidity ^0.8.19;

import {ERC165, IERC165} from "@openzeppelin-contracts-5.7.0/utils/introspection/ERC165.sol";
import {SafeERC20} from "@openzeppelin-contracts-5.7.0/token/ERC20/utils/SafeERC20.sol";
import {IERC20} from "@openzeppelin-contracts-5.7.0/token/ERC20/IERC20.sol";
import {ReentrancyGuard} from "@openzeppelin-contracts-5.7.0/utils/ReentrancyGuard.sol";
import {
    ON_FLASH_LOAN_CALLBACK_SUCCESS
} from "raindex-interface-0.1.14/src/interface/ierc3156/IERC3156FlashBorrower.sol";
import {IRaindexV7, TakeOrdersConfigV6, TaskV3} from "raindex-interface-0.1.14/src/interface/IRaindexV7.sol";
import {IERC3156FlashBorrower} from "raindex-interface-0.1.14/src/interface/ierc3156/IERC3156FlashBorrower.sol";
import {RaindexV7ArbCommon, BadRaindex} from "./RaindexV7ArbCommon.sol";
import {LibRaindexArb} from "../lib/LibRaindexArb.sol";
import {LibRaindexDeploy} from "../lib/deploy/LibRaindexDeploy.sol";
import {LibTOFUTokenDecimals} from "rain-tofu-erc20-decimals-0.1.5/src/lib/LibTOFUTokenDecimals.sol";
import {LibDecimalFloat} from "rain-math-float-0.3.12/src/lib/LibDecimalFloat.sol";

/// Thrown when the flash loan initiator is not this contract.
/// @param badInitiator The untrusted initiator of the flash loan.
error BadInitiator(address badInitiator);

/// Thrown when onFlashLoan is called by an address other than the deterministic
/// raindex deployment. Prevents malicious contracts from invoking the callback.
/// @param badLender The untrusted caller of onFlashLoan.
error BadLender(address badLender);

/// Thrown when the flash loan fails somehow.
error FlashLoanFailed();

/// Thrown when `arb5` is called with `IOIsInput == false`. The flash-loan amount
/// is the order output the taker receives, which `minimumIO` expresses only when
/// it is the taker input (`IOIsInput == true`).
error ArbRequiresInputIO();

/// @title RaindexV7FlashBorrower
/// @notice Abstract contract that liq-source specialized contracts can inherit
/// to provide flash loan based arbitrage against external liquidity sources to
/// fill raindex orders.
///
/// For example consider a simple order:
///
/// input = DAI
/// output = USDT
/// IORatio = 1.01
/// Order amount = 100
///
/// Assume external liq is offering 102 DAI per USDT so it exceeds the IO ratio
/// but the order itself has no way to interact with the external contract.
/// The `RaindexFlashBorrower` can:
///
/// - Flash loan 100 USDT from `Raindex`
/// - Sell the 100 USDT for 102 DAI on external liq
/// - Take the order, giving 101 DAI and paying down 100 USDT loan
/// - Keep 1 DAI profit
///
/// As this contract is expected to be cloned using a minimal proxy there will
/// be many copies of it in the wild. Each copy can be access gated by a Rain
/// expression that is evaluated before anything else happens in `arb`. There
/// are many reasons why this might be desirable:
/// - Regulatory reasons that restrict how an arb bot operator can interact with
///   orders and/or external liq.
/// - The arb operator wants to attempt to prevent front running by other bots.
/// - The arb operator may prefer a dedicated instance of the contract to make
///   it easier to track profits, etc.
abstract contract RaindexV7FlashBorrower is IERC3156FlashBorrower, ReentrancyGuard, ERC165, RaindexV7ArbCommon {
    using SafeERC20 for IERC20;

    constructor() {}

    /// @inheritdoc IERC165
    function supportsInterface(bytes4 interfaceId) public view virtual override returns (bool) {
        return interfaceId == type(IERC3156FlashBorrower).interfaceId || super.supportsInterface(interfaceId);
    }

    /// @dev Hook that inheriting contracts SHOULD override in order to achieve
    /// anything other than raising the ambient temperature of the room.
    /// `_exchange` is responsible for converting the flash loaned assets into
    /// the assets required to fill the orders. Generally this can only be
    /// achieved by interacting with an external liquidity source that is
    /// offering a better price than the orders require.
    /// @param takeOrders As per `arb`.
    /// @param exchangeData As per `arb`.
    //slither-disable-next-line dead-code
    function _exchange(TakeOrdersConfigV6 memory takeOrders, bytes memory exchangeData) internal virtual {}

    /// @inheritdoc IERC3156FlashBorrower
    function onFlashLoan(address initiator, address, uint256, uint256, bytes calldata data) external returns (bytes32) {
        // Only the deterministic raindex deployment may call this callback.
        if (msg.sender != LibRaindexDeploy.RAINDEX_DEPLOYED_ADDRESS) {
            revert BadLender(msg.sender);
        }
        // As per reference implementation.
        if (initiator != address(this)) {
            revert BadInitiator(initiator);
        }

        (TakeOrdersConfigV6 memory takeOrders, bytes memory exchangeData) =
            abi.decode(data, (TakeOrdersConfigV6, bytes));

        // Dispatch the `_exchange` hook to ensure we have the correct asset
        // type and amount to fill the orders.
        _exchange(takeOrders, exchangeData);

        // At this point `_exchange` should have sent the tokens required to match
        // the orders so take orders now.
        // We don't do anything with the total input/output amounts here because
        // the flash loan itself will take back what it needs, and we simply
        // keep anything left over according to active balances.
        //slither-disable-next-line unused-return
        IRaindexV7(msg.sender).takeOrders5(takeOrders);

        return ON_FLASH_LOAN_CALLBACK_SUCCESS;
    }

    /// Primary function to process arbitrage opportunities.
    /// The `_beforeArb` hook is called to validate the task before execution.
    /// If no task gating is configured, anyone can call this and should expect
    /// to be front run on the arb for any sufficiently profitable opportunity.
    ///
    /// A flash loan is taken from the raindex, then inside the callback
    /// `_exchange` converts the borrowed tokens into the tokens needed to fill
    /// the orders, and the orders are taken. After the flash loan repays,
    /// remaining assets are sent to the sender via `finalizeArb`.
    ///
    /// @param raindex The `Raindex` contract to take orders from and flash
    /// loan against. MUST be the deterministic deployment, else reverts
    /// `BadRaindex`.
    /// @param takeOrders As per `IRaindexV7.takeOrders5`.
    /// @param exchangeData Arbitrary bytes that will be passed to `_exchange`
    /// after the flash loan is taken. The inheriting contract is responsible
    /// for decoding this data and defining how it controls interactions with
    /// the external liquidity. For example, `GenericPoolRaindexV7FlashBorrower`
    /// uses this data as a literal encoded external call.
    /// @param task The task to evaluate after the arb completes.
    function arb5(
        IRaindexV7 raindex,
        TakeOrdersConfigV6 calldata takeOrders,
        bytes calldata exchangeData,
        TaskV3 calldata task
    ) external payable nonReentrant {
        _beforeArb(task);
        // Mimic what Raindex would do anyway if called with zero orders.
        if (takeOrders.orders.length == 0) {
            revert IRaindexV7.NoOrders();
        }
        if (address(raindex) != LibRaindexDeploy.RAINDEX_DEPLOYED_ADDRESS) {
            revert BadRaindex(address(raindex));
        }
        // The flash-loan amount below reads `minimumIO` as the order output the
        // taker receives, which only matches when `minimumIO` is the taker
        // input (`IOIsInput == true`).
        if (!takeOrders.IOIsInput) {
            revert ArbRequiresInputIO();
        }

        // Encode everything that will be used by the flash loan callback.
        bytes memory data = abi.encode(takeOrders, exchangeData);
        // The token we receive from taking the orders is what we will use to
        // repay the flash loan.
        address ordersOutputToken = takeOrders.orders[0].order.validOutputs[takeOrders.orders[0].outputIOIndex].token;
        address ordersInputToken = takeOrders.orders[0].order.validInputs[takeOrders.orders[0].inputIOIndex].token;

        uint8 inputDecimals = LibTOFUTokenDecimals.safeDecimalsForToken(ordersInputToken);
        uint8 outputDecimals = LibTOFUTokenDecimals.safeDecimalsForToken(ordersOutputToken);

        // We can't repay more than the minimum that the orders are going to
        // give us and there's no reason to borrow less.
        uint256 flashLoanAmount = LibDecimalFloat.toFixedDecimalLossless(takeOrders.minimumIO, outputDecimals);

        // Take the flash loan, which will in turn call `onFlashLoan`, which is
        // expected to process an exchange against external liq to pay back the
        // flash loan, cover the orders and remain in profit.
        IERC20(ordersInputToken).forceApprove(address(raindex), type(uint256).max);
        IERC20(ordersOutputToken).forceApprove(address(raindex), type(uint256).max);
        if (!raindex.flashLoan(this, ordersOutputToken, flashLoanAmount, data)) {
            revert FlashLoanFailed();
        }
        IERC20(ordersOutputToken).forceApprove(address(raindex), 0);
        IERC20(ordersInputToken).forceApprove(address(raindex), 0);

        LibRaindexArb.finalizeArb(task, ordersInputToken, inputDecimals, ordersOutputToken, outputDecimals);
    }
}
