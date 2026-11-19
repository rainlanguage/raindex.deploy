// SPDX-License-Identifier: LicenseRef-DCL-1.0
// SPDX-FileCopyrightText: Copyright (c) 2020 Rain Open Source Software Ltd
pragma solidity =0.8.25;

// The subgraph build reads out/DecimalFloat.sol/DecimalFloat.json, but raindex
// only imports LibDecimalFloat (not the concrete contract), so forge would not
// otherwise emit that ABI. Importing the concrete contract here forces forge to
// compile it.
import {DecimalFloat} from "rain-math-float-deploy-0.1.12/src/concrete/DecimalFloat.sol";
