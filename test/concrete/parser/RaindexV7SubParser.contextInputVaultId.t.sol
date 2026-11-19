// SPDX-License-Identifier: LicenseRef-DCL-1.0
// SPDX-FileCopyrightText: Copyright (c) 2020 Rain Open Source Software Ltd
pragma solidity =0.8.25;

import {RaindexV7SubParserContextTest} from "test/util/abstract/RaindexV7SubParserContextTest.sol";

contract RaindexV7SubParserContextInputVaultIdTest is RaindexV7SubParserContextTest {
    function word() internal pure override returns (string memory) {
        return "input-vault-id";
    }
}
