// SPDX-License-Identifier: LicenseRef-DCL-1.0
// SPDX-FileCopyrightText: Copyright (c) 2020 Rain Open Source Software Ltd
pragma solidity ^0.8.25;

/// @dev Every Raindex sub parser context word as a rainlang call expression.
library LibTestContextWords {
    function allWords() internal pure returns (string[] memory) {
        string[] memory words = new string[](31);
        words[0] = "sender()";
        words[1] = "raindex()";
        words[2] = "order-hash()";
        words[3] = "order-owner()";
        words[4] = "order-counterparty()";
        words[5] = "order-clearer()";
        words[6] = "calculated-max-output()";
        words[7] = "calculated-io-ratio()";
        words[8] = "input-token()";
        words[9] = "input-token-decimals()";
        words[10] = "input-vault-id()";
        words[11] = "input-vault-before()";
        words[12] = "input-vault-increase()";
        words[13] = "output-token()";
        words[14] = "output-token-decimals()";
        words[15] = "output-vault-id()";
        words[16] = "output-vault-before()";
        words[17] = "output-vault-decrease()";
        words[18] = "depositor()";
        words[19] = "deposit-token()";
        words[20] = "deposit-vault-id()";
        words[21] = "deposit-vault-before()";
        words[22] = "deposit-vault-after()";
        words[23] = "deposit-token-decimals()";
        words[24] = "withdrawer()";
        words[25] = "withdraw-token()";
        words[26] = "withdraw-vault-id()";
        words[27] = "withdraw-vault-before()";
        words[28] = "withdraw-vault-after()";
        words[29] = "withdraw-target-amount()";
        words[30] = "withdraw-token-decimals()";
        return words;
    }
}
