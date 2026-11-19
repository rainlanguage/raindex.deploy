// SPDX-License-Identifier: LicenseRef-DCL-1.0
// SPDX-FileCopyrightText: Copyright (c) 2020 Rain Open Source Software Ltd
pragma solidity =0.8.25;

import {ERC20} from "@openzeppelin-contracts-5.7.0/token/ERC20/ERC20.sol";

/// @dev Thrown by `RevertOnZeroTransferToken` when a zero-value transfer is
/// attempted.
error ZeroValueTransfer();

/// @dev ERC20 that reverts on any zero-amount transfer. Some real tokens behave
/// this way, which is exactly why `finalizeArb` guards its sweeps with
/// `if (balance > 0)`. A test holding a zero balance of this token proves the
/// guard skips the transfer rather than attempting a reverting zero transfer.
contract RevertOnZeroTransferToken is ERC20 {
    uint8 internal immutable iDecimals;

    constructor(string memory name, string memory symbol, uint8 decimals_) ERC20(name, symbol) {
        iDecimals = decimals_;
    }

    function decimals() public view override returns (uint8) {
        return iDecimals;
    }

    function mint(address to, uint256 amount) external {
        _mint(to, amount);
    }

    function transfer(address to, uint256 amount) public override returns (bool) {
        if (amount == 0) {
            revert ZeroValueTransfer();
        }
        return super.transfer(to, amount);
    }

    function transferFrom(address from, address to, uint256 amount) public override returns (bool) {
        if (amount == 0) {
            revert ZeroValueTransfer();
        }
        return super.transferFrom(from, to, amount);
    }
}
