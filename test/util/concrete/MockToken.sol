// SPDX-License-Identifier: LicenseRef-DCL-1.0
// SPDX-FileCopyrightText: Copyright (c) 2020 Rain Open Source Software Ltd
pragma solidity =0.8.25;

import {ERC20} from "@openzeppelin-contracts-5.7.0/token/ERC20/ERC20.sol";

contract MockToken is ERC20 {
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
}
