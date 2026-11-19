// SPDX-License-Identifier: LicenseRef-DCL-1.0
// SPDX-FileCopyrightText: Copyright (c) 2020 Rain Open Source Software Ltd
pragma solidity =0.8.25;

import {ERC20} from "@openzeppelin-contracts-5.7.0/token/ERC20/ERC20.sol";

contract TKN is ERC20 {
    constructor(address recipient, uint256 supply) ERC20("TKN", "TKN") {
        _mint(recipient, supply);
    }
}
