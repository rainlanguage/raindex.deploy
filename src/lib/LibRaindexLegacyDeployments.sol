// SPDX-License-Identifier: LicenseRef-DCL-1.0
// SPDX-FileCopyrightText: Copyright (c) 2020 Rain Open Source Software Ltd
pragma solidity ^0.8.25;

import {LibRainDeploy} from "rain-deploy-0.1.25/src/lib/LibRainDeploy.sol";
import {IOrderBookV3} from "raindex-interface-0.1.13/src/interface/deprecated/v3/IOrderBookV3.sol";
import {IOrderBookV4} from "raindex-interface-0.1.13/src/interface/deprecated/v4/IOrderBookV4.sol";
import {IOrderBookV5} from "raindex-interface-0.1.13/src/interface/deprecated/v5/IOrderBookV5.sol";
import {IRaindexV6} from "raindex-interface-0.1.13/src/interface/deprecated/v6/IRaindexV6.sol";

/// One raindex deployment on one network that no `src/generated/<tag>/` release covers.
struct RaindexLegacyDeployment {
    /// The network, by its rpc alias name.
    string network;
    /// The deployed raindex.
    address deployedAddress;
    /// The first block with code at `deployedAddress` on `network`.
    uint256 blockNumber;
    /// The ERC-165 interface id of the raindex interface the deployment implements.
    bytes4 interfaceId;
}

/// @title LibRaindexLegacyDeployments
/// @notice Every raindex deployment no release covers, ordered by network then block.
library LibRaindexLegacyDeployments {
    /// Every recorded legacy deployment.
    /// @return The recorded legacy deployments.
    function deployments() internal pure returns (RaindexLegacyDeployment[] memory) {
        RaindexLegacyDeployment[] memory records = new RaindexLegacyDeployment[](59);
        records[0] = RaindexLegacyDeployment({
            network: LibRainDeploy.ARBITRUM_ONE,
            deployedAddress: address(0x550878091b2B1506069F61ae59e3A5484Bca9166),
            blockNumber: 256576979,
            interfaceId: type(IOrderBookV4).interfaceId
        });
        records[1] = RaindexLegacyDeployment({
            network: LibRainDeploy.ARBITRUM_ONE,
            deployedAddress: address(0x59401C9302E79Eb8AC6aea659B8B3ae475715e86),
            blockNumber: 343433995,
            interfaceId: type(IOrderBookV4).interfaceId
        });
        records[2] = RaindexLegacyDeployment({
            network: LibRainDeploy.ARBITRUM_ONE,
            deployedAddress: address(0x2f209e5b67A33B8fE96E28f24628dF6Da301c8eB),
            blockNumber: 352866209,
            interfaceId: type(IOrderBookV5).interfaceId
        });
        records[3] = RaindexLegacyDeployment({
            network: LibRainDeploy.ARBITRUM_ONE,
            deployedAddress: address(0xF6FdCB30FD027C73cBacD54E22f02987E3116eBc),
            blockNumber: 353562229,
            interfaceId: type(IOrderBookV4).interfaceId
        });
        records[4] = RaindexLegacyDeployment({
            network: LibRainDeploy.ARBITRUM_ONE,
            deployedAddress: address(0x783b82f0fBF6743882072AE2393B108F5938898B),
            blockNumber: 367977688,
            interfaceId: type(IOrderBookV4).interfaceId
        });
        records[5] = RaindexLegacyDeployment({
            network: LibRainDeploy.ARBITRUM_ONE,
            deployedAddress: address(0xbC176C5884CA778b91729C0092245905Df28Cd8f),
            blockNumber: 368726460,
            interfaceId: type(IOrderBookV4).interfaceId
        });
        records[6] = RaindexLegacyDeployment({
            network: LibRainDeploy.ARBITRUM_ONE,
            deployedAddress: address(0xE3989Ea7486c0F418C764e6c511e86f6E8830FAb),
            blockNumber: 369669314,
            interfaceId: type(IOrderBookV4).interfaceId
        });
        records[7] = RaindexLegacyDeployment({
            network: LibRainDeploy.ARBITRUM_ONE,
            deployedAddress: address(0x85e440C297f267332c34A88E0d085162a96475D2),
            blockNumber: 369790804,
            interfaceId: type(IOrderBookV4).interfaceId
        });
        records[8] = RaindexLegacyDeployment({
            network: LibRainDeploy.ARBITRUM_ONE,
            deployedAddress: address(0x93E7298a2c7d05C151ba4572140b8b52e06792bD),
            blockNumber: 376964692,
            interfaceId: type(IOrderBookV5).interfaceId
        });
        records[9] = RaindexLegacyDeployment({
            network: LibRainDeploy.ARBITRUM_ONE,
            deployedAddress: address(0x8a3C8E610d827093F7437E0C45EFa648563c0dDA),
            blockNumber: 379008108,
            interfaceId: type(IOrderBookV5).interfaceId
        });
        records[10] = RaindexLegacyDeployment({
            network: LibRainDeploy.ARBITRUM_ONE,
            deployedAddress: address(0x8dF8075e4077dAbf1e95F49059E4C1eEA33094ab),
            blockNumber: 380203894,
            interfaceId: type(IOrderBookV5).interfaceId
        });
        records[11] = RaindexLegacyDeployment({
            network: LibRainDeploy.ARBITRUM_ONE,
            deployedAddress: address(0x7242ce1ffe34297921c54cD7f9343c411E1E7759),
            blockNumber: 441612693,
            interfaceId: type(IRaindexV6).interfaceId
        });
        records[12] = RaindexLegacyDeployment({
            network: LibRainDeploy.ARBITRUM_ONE,
            deployedAddress: address(0xa560e58C77B8A841f80B4741Ad08E9260Edcab9F),
            blockNumber: 467988520,
            interfaceId: type(IRaindexV6).interfaceId
        });
        records[13] = RaindexLegacyDeployment({
            network: LibRainDeploy.ARBITRUM_ONE,
            deployedAddress: address(0x83507d7E14556A1822E6959982BebA6024245FA8),
            blockNumber: 469250595,
            interfaceId: type(IRaindexV6).interfaceId
        });
        records[14] = RaindexLegacyDeployment({
            network: LibRainDeploy.ARBITRUM_ONE,
            deployedAddress: address(0x3Ee54EbB1734DA14852C06A3Be1CdEebA796B0a3),
            blockNumber: 469346937,
            interfaceId: type(IRaindexV6).interfaceId
        });
        records[15] = RaindexLegacyDeployment({
            network: LibRainDeploy.ARBITRUM_ONE,
            deployedAddress: address(0xb05D73E6BCc26AEB5b67Ff68C6E9C6151073e3cE),
            blockNumber: 469385437,
            interfaceId: type(IRaindexV6).interfaceId
        });
        records[16] = RaindexLegacyDeployment({
            network: LibRainDeploy.ARBITRUM_ONE,
            deployedAddress: address(0x13b8f830e347a51778652C86cf3419b3E1Ab1205),
            blockNumber: 472046415,
            interfaceId: type(IRaindexV6).interfaceId
        });
        records[17] = RaindexLegacyDeployment({
            network: LibRainDeploy.BASE,
            deployedAddress: address(0xd2938E7c9fe3597F78832CE780Feb61945c377d7),
            blockNumber: 19033330,
            interfaceId: type(IOrderBookV4).interfaceId
        });
        records[18] = RaindexLegacyDeployment({
            network: LibRainDeploy.BASE,
            deployedAddress: address(0xF6FdCB30FD027C73cBacD54E22f02987E3116eBc),
            blockNumber: 31080825,
            interfaceId: type(IOrderBookV4).interfaceId
        });
        records[19] = RaindexLegacyDeployment({
            network: LibRainDeploy.BASE,
            deployedAddress: address(0x7692BA8446Bb8B3140A2c02df073080BeD0a7F8E),
            blockNumber: 32346650,
            interfaceId: type(IOrderBookV4).interfaceId
        });
        records[20] = RaindexLegacyDeployment({
            network: LibRainDeploy.BASE,
            deployedAddress: address(0x5F0EF3256aC5d2C57ac8550d3F0d5Aa914be84Df),
            blockNumber: 34148305,
            interfaceId: type(IOrderBookV4).interfaceId
        });
        records[21] = RaindexLegacyDeployment({
            network: LibRainDeploy.BASE,
            deployedAddress: address(0x84c6e7F5A1e5dD89594Cc25BEf4722A1b8871aE6),
            blockNumber: 34241018,
            interfaceId: type(IOrderBookV4).interfaceId
        });
        records[22] = RaindexLegacyDeployment({
            network: LibRainDeploy.BASE,
            deployedAddress: address(0x254Aa4443189A6393F77A17f299EDEE2a5b49583),
            blockNumber: 34359390,
            interfaceId: type(IOrderBookV4).interfaceId
        });
        records[23] = RaindexLegacyDeployment({
            network: LibRainDeploy.BASE,
            deployedAddress: address(0x881cf4c0764e733d9C387f3858eE87CcA04AFFe0),
            blockNumber: 34374432,
            interfaceId: type(IOrderBookV4).interfaceId
        });
        records[24] = RaindexLegacyDeployment({
            network: LibRainDeploy.BASE,
            deployedAddress: address(0x52CEB8eBEf648744fFDDE89F7Bc9C3aC35944775),
            blockNumber: 36667253,
            interfaceId: type(IOrderBookV5).interfaceId
        });
        records[25] = RaindexLegacyDeployment({
            network: LibRainDeploy.BASE,
            deployedAddress: address(0xFe2411CDa193D9E4e83A5c234C7Fd320101883aC),
            blockNumber: 40578309,
            interfaceId: type(IOrderBookV5).interfaceId
        });
        records[26] = RaindexLegacyDeployment({
            network: LibRainDeploy.BASE,
            deployedAddress: address(0x7D018e8e7a234CA07cAb212B325A8d0A9A362CA6),
            blockNumber: 40583757,
            interfaceId: type(IRaindexV6).interfaceId
        });
        records[27] = RaindexLegacyDeployment({
            network: LibRainDeploy.BASE,
            deployedAddress: address(0x77Ca4947dce161e122Eb690D475448f092a7d7EB),
            blockNumber: 40754388,
            interfaceId: type(IRaindexV6).interfaceId
        });
        records[28] = RaindexLegacyDeployment({
            network: LibRainDeploy.BASE,
            deployedAddress: address(0x498Ff70C5f67e63e53b74551DE31387cf2813986),
            blockNumber: 41134692,
            interfaceId: type(IRaindexV6).interfaceId
        });
        records[29] = RaindexLegacyDeployment({
            network: LibRainDeploy.BASE,
            deployedAddress: address(0xe522cB4a5fCb2eb31a52Ff41a4653d85A4fd7C9D),
            blockNumber: 41747644,
            interfaceId: type(IRaindexV6).interfaceId
        });
        records[30] = RaindexLegacyDeployment({
            network: LibRainDeploy.BASE,
            deployedAddress: address(0x7242ce1ffe34297921c54cD7f9343c411E1E7759),
            blockNumber: 43339885,
            interfaceId: type(IRaindexV6).interfaceId
        });
        records[31] = RaindexLegacyDeployment({
            network: LibRainDeploy.BASE,
            deployedAddress: address(0xa560e58C77B8A841f80B4741Ad08E9260Edcab9F),
            blockNumber: 46645657,
            interfaceId: type(IRaindexV6).interfaceId
        });
        records[32] = RaindexLegacyDeployment({
            network: LibRainDeploy.BASE,
            deployedAddress: address(0x83507d7E14556A1822E6959982BebA6024245FA8),
            blockNumber: 46804282,
            interfaceId: type(IRaindexV6).interfaceId
        });
        records[33] = RaindexLegacyDeployment({
            network: LibRainDeploy.BASE,
            deployedAddress: address(0x3Ee54EbB1734DA14852C06A3Be1CdEebA796B0a3),
            blockNumber: 46816305,
            interfaceId: type(IRaindexV6).interfaceId
        });
        records[34] = RaindexLegacyDeployment({
            network: LibRainDeploy.BASE,
            deployedAddress: address(0xb05D73E6BCc26AEB5b67Ff68C6E9C6151073e3cE),
            blockNumber: 46821107,
            interfaceId: type(IRaindexV6).interfaceId
        });
        records[35] = RaindexLegacyDeployment({
            network: LibRainDeploy.BASE,
            deployedAddress: address(0x13b8f830e347a51778652C86cf3419b3E1Ab1205),
            blockNumber: 47154607,
            interfaceId: type(IRaindexV6).interfaceId
        });
        records[36] = RaindexLegacyDeployment({
            network: LibRainDeploy.BASE_SEPOLIA,
            deployedAddress: address(0x7242ce1ffe34297921c54cD7f9343c411E1E7759),
            blockNumber: 38850417,
            interfaceId: type(IRaindexV6).interfaceId
        });
        records[37] = RaindexLegacyDeployment({
            network: LibRainDeploy.BSC,
            deployedAddress: address(0xd2938E7c9fe3597F78832CE780Feb61945c377d7),
            blockNumber: 42510318,
            interfaceId: type(IOrderBookV4).interfaceId
        });
        records[38] = RaindexLegacyDeployment({
            network: LibRainDeploy.ETHEREUM,
            deployedAddress: address(0x0eA6d458488d1cf51695e1D6e4744e6FB715d37C),
            blockNumber: 21041924,
            interfaceId: type(IOrderBookV4).interfaceId
        });
        records[39] = RaindexLegacyDeployment({
            network: LibRainDeploy.FLARE,
            deployedAddress: address(0xb06202aA3Fe7d85171fB7aA5f17011d17E63f382),
            blockNumber: 22109423,
            interfaceId: type(IOrderBookV3).interfaceId
        });
        records[40] = RaindexLegacyDeployment({
            network: LibRainDeploy.FLARE,
            deployedAddress: address(0xCEe8Cd002F151A536394E564b84076c41bBBcD4d),
            blockNumber: 29307299,
            interfaceId: type(IOrderBookV4).interfaceId
        });
        records[41] = RaindexLegacyDeployment({
            network: LibRainDeploy.FLARE,
            deployedAddress: address(0x8Dbc50485e7C6994923B9A2c603a2B6b2d8Eef64),
            blockNumber: 49782578,
            interfaceId: type(IOrderBookV5).interfaceId
        });
        records[42] = RaindexLegacyDeployment({
            network: LibRainDeploy.FLARE,
            deployedAddress: address(0x7242ce1ffe34297921c54cD7f9343c411E1E7759),
            blockNumber: 56972130,
            interfaceId: type(IRaindexV6).interfaceId
        });
        records[43] = RaindexLegacyDeployment({
            network: LibRainDeploy.FLARE,
            deployedAddress: address(0xa560e58C77B8A841f80B4741Ad08E9260Edcab9F),
            blockNumber: 61847874,
            interfaceId: type(IRaindexV6).interfaceId
        });
        records[44] = RaindexLegacyDeployment({
            network: LibRainDeploy.FLARE,
            deployedAddress: address(0x83507d7E14556A1822E6959982BebA6024245FA8),
            blockNumber: 62094702,
            interfaceId: type(IRaindexV6).interfaceId
        });
        records[45] = RaindexLegacyDeployment({
            network: LibRainDeploy.FLARE,
            deployedAddress: address(0x3Ee54EbB1734DA14852C06A3Be1CdEebA796B0a3),
            blockNumber: 62113357,
            interfaceId: type(IRaindexV6).interfaceId
        });
        records[46] = RaindexLegacyDeployment({
            network: LibRainDeploy.FLARE,
            deployedAddress: address(0xb05D73E6BCc26AEB5b67Ff68C6E9C6151073e3cE),
            blockNumber: 62120965,
            interfaceId: type(IRaindexV6).interfaceId
        });
        records[47] = RaindexLegacyDeployment({
            network: LibRainDeploy.FLARE,
            deployedAddress: address(0x13b8f830e347a51778652C86cf3419b3E1Ab1205),
            blockNumber: 62642618,
            interfaceId: type(IRaindexV6).interfaceId
        });
        records[48] = RaindexLegacyDeployment({
            network: "linea",
            deployedAddress: address(0x22410e2a46261a1B1e3899a072f303022801C764),
            blockNumber: 10147523,
            interfaceId: type(IOrderBookV4).interfaceId
        });
        records[49] = RaindexLegacyDeployment({
            network: "matchain",
            deployedAddress: address(0x40312EDAB8Fe65091354172ad79e9459f21094E2),
            blockNumber: 3976602,
            interfaceId: type(IOrderBookV4).interfaceId
        });
        records[50] = RaindexLegacyDeployment({
            network: LibRainDeploy.POLYGON,
            deployedAddress: address(0xc95A5f8eFe14d7a20BD2E5BAFEC4E71f8Ce0B9A6),
            blockNumber: 54697866,
            interfaceId: type(IOrderBookV3).interfaceId
        });
        records[51] = RaindexLegacyDeployment({
            network: LibRainDeploy.POLYGON,
            deployedAddress: address(0x7D2f700b1f6FD75734824EA4578960747bdF269A),
            blockNumber: 62036116,
            interfaceId: type(IOrderBookV4).interfaceId
        });
        records[52] = RaindexLegacyDeployment({
            network: LibRainDeploy.POLYGON,
            deployedAddress: address(0x8a3C8E610d827093F7437E0C45EFa648563c0dDA),
            blockNumber: 76772162,
            interfaceId: type(IOrderBookV5).interfaceId
        });
        records[53] = RaindexLegacyDeployment({
            network: LibRainDeploy.POLYGON,
            deployedAddress: address(0x7242ce1ffe34297921c54cD7f9343c411E1E7759),
            blockNumber: 84174550,
            interfaceId: type(IRaindexV6).interfaceId
        });
        records[54] = RaindexLegacyDeployment({
            network: LibRainDeploy.POLYGON,
            deployedAddress: address(0xa560e58C77B8A841f80B4741Ad08E9260Edcab9F),
            blockNumber: 87623401,
            interfaceId: type(IRaindexV6).interfaceId
        });
        records[55] = RaindexLegacyDeployment({
            network: LibRainDeploy.POLYGON,
            deployedAddress: address(0x83507d7E14556A1822E6959982BebA6024245FA8),
            blockNumber: 87804686,
            interfaceId: type(IRaindexV6).interfaceId
        });
        records[56] = RaindexLegacyDeployment({
            network: LibRainDeploy.POLYGON,
            deployedAddress: address(0x3Ee54EbB1734DA14852C06A3Be1CdEebA796B0a3),
            blockNumber: 87818427,
            interfaceId: type(IRaindexV6).interfaceId
        });
        records[57] = RaindexLegacyDeployment({
            network: LibRainDeploy.POLYGON,
            deployedAddress: address(0xb05D73E6BCc26AEB5b67Ff68C6E9C6151073e3cE),
            blockNumber: 87823915,
            interfaceId: type(IRaindexV6).interfaceId
        });
        records[58] = RaindexLegacyDeployment({
            network: LibRainDeploy.POLYGON,
            deployedAddress: address(0x13b8f830e347a51778652C86cf3419b3E1Ab1205),
            blockNumber: 88264952,
            interfaceId: type(IRaindexV6).interfaceId
        });
        return records;
    }
}
