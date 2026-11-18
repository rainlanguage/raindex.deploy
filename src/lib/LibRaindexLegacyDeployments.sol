// SPDX-License-Identifier: LicenseRef-DCL-1.0
// SPDX-FileCopyrightText: Copyright (c) 2020 Rain Open Source Software Ltd
pragma solidity ^0.8.25;

import {LibRainDeploy} from "rain-deploy-0.1.33/src/lib/LibRainDeploy.sol";

/// One raindex deployment on one network.
struct RaindexDeployment {
    /// The network, by its rpc alias name.
    string network;
    /// The deployed raindex.
    address deployedAddress;
    /// The raindex interface version (3 to 7) its runtime code implements,
    /// which selects the event signatures a consumer decodes it with.
    uint256 interfaceVersion;
    /// The first block with code at `deployedAddress` on `network`.
    uint256 blockNumber;
}

/// @title LibRaindexLegacyDeployments
/// @notice Every raindex deployment that no `src/generated/<tag>/` release
/// records, ordered by network then block: the deployments with no other
/// record. Released deployments are read from the released-suites libs, never
/// listed here. Each row is checked on chain: code at the address, the
/// version's deposit selector in that code, and the first block with code.
library LibRaindexLegacyDeployments {
    /// Every legacy deployment.
    /// @return The legacy deployments.
    function deployments() internal pure returns (RaindexDeployment[] memory) {
        RaindexDeployment[] memory records = new RaindexDeployment[](139);
        records[0] = RaindexDeployment({
            network: LibRainDeploy.ARBITRUM_ONE,
            deployedAddress: address(0xee873E21F8a6A256cb9f53BE491E569eA6cdd63E),
            interfaceVersion: 3,
            blockNumber: 190719437
        });
        records[1] = RaindexDeployment({
            network: LibRainDeploy.ARBITRUM_ONE,
            deployedAddress: address(0x90CAF23eA7E507BB722647B0674e50D8d6468234),
            interfaceVersion: 3,
            blockNumber: 190892465
        });
        records[2] = RaindexDeployment({
            network: LibRainDeploy.ARBITRUM_ONE,
            deployedAddress: address(0x550878091b2B1506069F61ae59e3A5484Bca9166),
            interfaceVersion: 4,
            blockNumber: 256576979
        });
        records[3] = RaindexDeployment({
            network: LibRainDeploy.ARBITRUM_ONE,
            deployedAddress: address(0x59401C9302E79Eb8AC6aea659B8B3ae475715e86),
            interfaceVersion: 4,
            blockNumber: 343433995
        });
        records[4] = RaindexDeployment({
            network: LibRainDeploy.ARBITRUM_ONE,
            deployedAddress: address(0x2f209e5b67A33B8fE96E28f24628dF6Da301c8eB),
            interfaceVersion: 5,
            blockNumber: 352866209
        });
        records[5] = RaindexDeployment({
            network: LibRainDeploy.ARBITRUM_ONE,
            deployedAddress: address(0xF6FdCB30FD027C73cBacD54E22f02987E3116eBc),
            interfaceVersion: 4,
            blockNumber: 353562229
        });
        records[6] = RaindexDeployment({
            network: LibRainDeploy.ARBITRUM_ONE,
            deployedAddress: address(0x783b82f0fBF6743882072AE2393B108F5938898B),
            interfaceVersion: 4,
            blockNumber: 367977688
        });
        records[7] = RaindexDeployment({
            network: LibRainDeploy.ARBITRUM_ONE,
            deployedAddress: address(0xbC176C5884CA778b91729C0092245905Df28Cd8f),
            interfaceVersion: 4,
            blockNumber: 368726460
        });
        records[8] = RaindexDeployment({
            network: LibRainDeploy.ARBITRUM_ONE,
            deployedAddress: address(0xE3989Ea7486c0F418C764e6c511e86f6E8830FAb),
            interfaceVersion: 4,
            blockNumber: 369669314
        });
        records[9] = RaindexDeployment({
            network: LibRainDeploy.ARBITRUM_ONE,
            deployedAddress: address(0x85e440C297f267332c34A88E0d085162a96475D2),
            interfaceVersion: 4,
            blockNumber: 369790804
        });
        records[10] = RaindexDeployment({
            network: LibRainDeploy.ARBITRUM_ONE,
            deployedAddress: address(0x93E7298a2c7d05C151ba4572140b8b52e06792bD),
            interfaceVersion: 5,
            blockNumber: 376964692
        });
        records[11] = RaindexDeployment({
            network: LibRainDeploy.ARBITRUM_ONE,
            deployedAddress: address(0x8a3C8E610d827093F7437E0C45EFa648563c0dDA),
            interfaceVersion: 5,
            blockNumber: 379008108
        });
        records[12] = RaindexDeployment({
            network: LibRainDeploy.ARBITRUM_ONE,
            deployedAddress: address(0xb7F3547682caA58dd7a356E10c522E50bf48157E),
            interfaceVersion: 5,
            blockNumber: 379698268
        });
        records[13] = RaindexDeployment({
            network: LibRainDeploy.ARBITRUM_ONE,
            deployedAddress: address(0x8dF8075e4077dAbf1e95F49059E4C1eEA33094ab),
            interfaceVersion: 5,
            blockNumber: 380203894
        });
        records[14] = RaindexDeployment({
            network: LibRainDeploy.ARBITRUM_ONE,
            deployedAddress: address(0x7242ce1ffe34297921c54cD7f9343c411E1E7759),
            interfaceVersion: 6,
            blockNumber: 441612693
        });
        records[15] = RaindexDeployment({
            network: LibRainDeploy.ARBITRUM_ONE,
            deployedAddress: address(0xa560e58C77B8A841f80B4741Ad08E9260Edcab9F),
            interfaceVersion: 6,
            blockNumber: 467988520
        });
        records[16] = RaindexDeployment({
            network: LibRainDeploy.ARBITRUM_ONE,
            deployedAddress: address(0x83507d7E14556A1822E6959982BebA6024245FA8),
            interfaceVersion: 6,
            blockNumber: 469250595
        });
        records[17] = RaindexDeployment({
            network: LibRainDeploy.ARBITRUM_ONE,
            deployedAddress: address(0x3Ee54EbB1734DA14852C06A3Be1CdEebA796B0a3),
            interfaceVersion: 6,
            blockNumber: 469346937
        });
        records[18] = RaindexDeployment({
            network: LibRainDeploy.ARBITRUM_ONE,
            deployedAddress: address(0xE8421069D1F56cC1207E6Ec21341BcFD044B8cd5),
            interfaceVersion: 6,
            blockNumber: 469367388
        });
        records[19] = RaindexDeployment({
            network: LibRainDeploy.ARBITRUM_ONE,
            deployedAddress: address(0xb05D73E6BCc26AEB5b67Ff68C6E9C6151073e3cE),
            interfaceVersion: 6,
            blockNumber: 469385437
        });
        records[20] = RaindexDeployment({
            network: LibRainDeploy.ARBITRUM_ONE,
            deployedAddress: address(0x4F4527919DA654C1718Ca6Ef194F6547226685f6),
            interfaceVersion: 6,
            blockNumber: 469964122
        });
        records[21] = RaindexDeployment({
            network: LibRainDeploy.ARBITRUM_ONE,
            deployedAddress: address(0xE0203890383Ca9C0c263f42a51C4C95211Cee6F9),
            interfaceVersion: 6,
            blockNumber: 471755491
        });
        records[22] = RaindexDeployment({
            network: LibRainDeploy.ARBITRUM_ONE,
            deployedAddress: address(0x025e93Be5c1780240C11E34B3387266754017D20),
            interfaceVersion: 6,
            blockNumber: 471992531
        });
        records[23] = RaindexDeployment({
            network: LibRainDeploy.ARBITRUM_ONE,
            deployedAddress: address(0x13b8f830e347a51778652C86cf3419b3E1Ab1205),
            interfaceVersion: 6,
            blockNumber: 472046415
        });
        records[24] = RaindexDeployment({
            network: LibRainDeploy.ARBITRUM_ONE,
            deployedAddress: address(0x86594ac4319230870C6E587f4ACa48FAb575619e),
            interfaceVersion: 6,
            blockNumber: 473030678
        });
        records[25] = RaindexDeployment({
            network: LibRainDeploy.ARBITRUM_ONE,
            deployedAddress: address(0x7992F7A3044258c9a200835970D5bFf275Efe5e7),
            interfaceVersion: 6,
            blockNumber: 473863630
        });
        records[26] = RaindexDeployment({
            network: LibRainDeploy.ARBITRUM_ONE,
            deployedAddress: address(0x1115EBC9C82F074454F6AC28f123B0684A453aF7),
            interfaceVersion: 6,
            blockNumber: 473877912
        });
        records[27] = RaindexDeployment({
            network: LibRainDeploy.BASE,
            deployedAddress: address(0xF77b3c3f61af5a3cE7f7CE3cfFc117491104432E),
            interfaceVersion: 3,
            blockNumber: 11901579
        });
        records[28] = RaindexDeployment({
            network: LibRainDeploy.BASE,
            deployedAddress: address(0x2AeE87D75CD000583DAEC7A28db103B1c0c18b76),
            interfaceVersion: 3,
            blockNumber: 11901852
        });
        records[29] = RaindexDeployment({
            network: LibRainDeploy.BASE,
            deployedAddress: address(0x5Cf7d0a8c61c8dcC6b0ECB281dF1C17264C2A517),
            interfaceVersion: 3,
            blockNumber: 15045541
        });
        records[30] = RaindexDeployment({
            network: LibRainDeploy.BASE,
            deployedAddress: address(0xA2f56F8F74B7d04d61f281BE6576b6155581dcBA),
            interfaceVersion: 4,
            blockNumber: 16572539
        });
        records[31] = RaindexDeployment({
            network: LibRainDeploy.BASE,
            deployedAddress: address(0x7A44459893F99b9d9a92d488eb5d16E4090f0545),
            interfaceVersion: 4,
            blockNumber: 18307547
        });
        records[32] = RaindexDeployment({
            network: LibRainDeploy.BASE,
            deployedAddress: address(0x80DE00e3cA96AE0569426A1bb1Ae22CD4181dE6F),
            interfaceVersion: 4,
            blockNumber: 18689505
        });
        records[33] = RaindexDeployment({
            network: LibRainDeploy.BASE,
            deployedAddress: address(0x32aCbdF51abe567C91b7a5cd5E52024a5Ca56844),
            interfaceVersion: 4,
            blockNumber: 18831295
        });
        records[34] = RaindexDeployment({
            network: LibRainDeploy.BASE,
            deployedAddress: address(0xd2938E7c9fe3597F78832CE780Feb61945c377d7),
            interfaceVersion: 4,
            blockNumber: 19033330
        });
        records[35] = RaindexDeployment({
            network: LibRainDeploy.BASE,
            deployedAddress: address(0xF6FdCB30FD027C73cBacD54E22f02987E3116eBc),
            interfaceVersion: 4,
            blockNumber: 31080825
        });
        records[36] = RaindexDeployment({
            network: LibRainDeploy.BASE,
            deployedAddress: address(0x7692BA8446Bb8B3140A2c02df073080BeD0a7F8E),
            interfaceVersion: 4,
            blockNumber: 32346650
        });
        records[37] = RaindexDeployment({
            network: LibRainDeploy.BASE,
            deployedAddress: address(0x5F0EF3256aC5d2C57ac8550d3F0d5Aa914be84Df),
            interfaceVersion: 4,
            blockNumber: 34148305
        });
        records[38] = RaindexDeployment({
            network: LibRainDeploy.BASE,
            deployedAddress: address(0x84c6e7F5A1e5dD89594Cc25BEf4722A1b8871aE6),
            interfaceVersion: 4,
            blockNumber: 34241018
        });
        records[39] = RaindexDeployment({
            network: LibRainDeploy.BASE,
            deployedAddress: address(0x254Aa4443189A6393F77A17f299EDEE2a5b49583),
            interfaceVersion: 4,
            blockNumber: 34359390
        });
        records[40] = RaindexDeployment({
            network: LibRainDeploy.BASE,
            deployedAddress: address(0x881cf4c0764e733d9C387f3858eE87CcA04AFFe0),
            interfaceVersion: 4,
            blockNumber: 34374432
        });
        records[41] = RaindexDeployment({
            network: LibRainDeploy.BASE,
            deployedAddress: address(0x6B36e02409BD60cA5C17049c7188de180857902D),
            interfaceVersion: 5,
            blockNumber: 36615601
        });
        records[42] = RaindexDeployment({
            network: LibRainDeploy.BASE,
            deployedAddress: address(0x52CEB8eBEf648744fFDDE89F7Bc9C3aC35944775),
            interfaceVersion: 5,
            blockNumber: 36667253
        });
        records[43] = RaindexDeployment({
            network: LibRainDeploy.BASE,
            deployedAddress: address(0xAa0F54aAc519A79B0a9a167a47F4dB0194df418f),
            interfaceVersion: 5,
            blockNumber: 37682786
        });
        records[44] = RaindexDeployment({
            network: LibRainDeploy.BASE,
            deployedAddress: address(0x3EFA7f890FFca73B7313A0c6B7Baed4949c1Cf8F),
            interfaceVersion: 5,
            blockNumber: 40575884
        });
        records[45] = RaindexDeployment({
            network: LibRainDeploy.BASE,
            deployedAddress: address(0xFe2411CDa193D9E4e83A5c234C7Fd320101883aC),
            interfaceVersion: 5,
            blockNumber: 40578309
        });
        records[46] = RaindexDeployment({
            network: LibRainDeploy.BASE,
            deployedAddress: address(0x7D018e8e7a234CA07cAb212B325A8d0A9A362CA6),
            interfaceVersion: 6,
            blockNumber: 40583757
        });
        records[47] = RaindexDeployment({
            network: LibRainDeploy.BASE,
            deployedAddress: address(0x77Ca4947dce161e122Eb690D475448f092a7d7EB),
            interfaceVersion: 6,
            blockNumber: 40754388
        });
        records[48] = RaindexDeployment({
            network: LibRainDeploy.BASE,
            deployedAddress: address(0x498Ff70C5f67e63e53b74551DE31387cf2813986),
            interfaceVersion: 6,
            blockNumber: 41134692
        });
        records[49] = RaindexDeployment({
            network: LibRainDeploy.BASE,
            deployedAddress: address(0xe522cB4a5fCb2eb31a52Ff41a4653d85A4fd7C9D),
            interfaceVersion: 6,
            blockNumber: 41747644
        });
        records[50] = RaindexDeployment({
            network: LibRainDeploy.BASE,
            deployedAddress: address(0x7242ce1ffe34297921c54cD7f9343c411E1E7759),
            interfaceVersion: 6,
            blockNumber: 43339885
        });
        records[51] = RaindexDeployment({
            network: LibRainDeploy.BASE,
            deployedAddress: address(0xa560e58C77B8A841f80B4741Ad08E9260Edcab9F),
            interfaceVersion: 6,
            blockNumber: 46645657
        });
        records[52] = RaindexDeployment({
            network: LibRainDeploy.BASE,
            deployedAddress: address(0x83507d7E14556A1822E6959982BebA6024245FA8),
            interfaceVersion: 6,
            blockNumber: 46804282
        });
        records[53] = RaindexDeployment({
            network: LibRainDeploy.BASE,
            deployedAddress: address(0x3Ee54EbB1734DA14852C06A3Be1CdEebA796B0a3),
            interfaceVersion: 6,
            blockNumber: 46816305
        });
        records[54] = RaindexDeployment({
            network: LibRainDeploy.BASE,
            deployedAddress: address(0xE8421069D1F56cC1207E6Ec21341BcFD044B8cd5),
            interfaceVersion: 6,
            blockNumber: 46818855
        });
        records[55] = RaindexDeployment({
            network: LibRainDeploy.BASE,
            deployedAddress: address(0xb05D73E6BCc26AEB5b67Ff68C6E9C6151073e3cE),
            interfaceVersion: 6,
            blockNumber: 46821107
        });
        records[56] = RaindexDeployment({
            network: LibRainDeploy.BASE,
            deployedAddress: address(0x4F4527919DA654C1718Ca6Ef194F6547226685f6),
            interfaceVersion: 6,
            blockNumber: 46893385
        });
        records[57] = RaindexDeployment({
            network: LibRainDeploy.BASE,
            deployedAddress: address(0xE0203890383Ca9C0c263f42a51C4C95211Cee6F9),
            interfaceVersion: 6,
            blockNumber: 47118075
        });
        records[58] = RaindexDeployment({
            network: LibRainDeploy.BASE,
            deployedAddress: address(0x025e93Be5c1780240C11E34B3387266754017D20),
            interfaceVersion: 6,
            blockNumber: 47147858
        });
        records[59] = RaindexDeployment({
            network: LibRainDeploy.BASE,
            deployedAddress: address(0x13b8f830e347a51778652C86cf3419b3E1Ab1205),
            interfaceVersion: 6,
            blockNumber: 47154607
        });
        records[60] = RaindexDeployment({
            network: LibRainDeploy.BASE,
            deployedAddress: address(0x86594ac4319230870C6E587f4ACa48FAb575619e),
            interfaceVersion: 6,
            blockNumber: 47278140
        });
        records[61] = RaindexDeployment({
            network: LibRainDeploy.BASE,
            deployedAddress: address(0x7992F7A3044258c9a200835970D5bFf275Efe5e7),
            interfaceVersion: 6,
            blockNumber: 47383088
        });
        records[62] = RaindexDeployment({
            network: LibRainDeploy.BASE,
            deployedAddress: address(0x1115EBC9C82F074454F6AC28f123B0684A453aF7),
            interfaceVersion: 6,
            blockNumber: 47384882
        });
        records[63] = RaindexDeployment({
            network: LibRainDeploy.BASE_SEPOLIA,
            deployedAddress: address(0x7242ce1ffe34297921c54cD7f9343c411E1E7759),
            interfaceVersion: 6,
            blockNumber: 38850417
        });
        records[64] = RaindexDeployment({
            network: LibRainDeploy.BASE_SEPOLIA,
            deployedAddress: address(0xa560e58C77B8A841f80B4741Ad08E9260Edcab9F),
            interfaceVersion: 6,
            blockNumber: 42156188
        });
        records[65] = RaindexDeployment({
            network: LibRainDeploy.BASE_SEPOLIA,
            deployedAddress: address(0x83507d7E14556A1822E6959982BebA6024245FA8),
            interfaceVersion: 6,
            blockNumber: 42314813
        });
        records[66] = RaindexDeployment({
            network: LibRainDeploy.BASE_SEPOLIA,
            deployedAddress: address(0x3Ee54EbB1734DA14852C06A3Be1CdEebA796B0a3),
            interfaceVersion: 6,
            blockNumber: 42326836
        });
        records[67] = RaindexDeployment({
            network: LibRainDeploy.BASE_SEPOLIA,
            deployedAddress: address(0xE8421069D1F56cC1207E6Ec21341BcFD044B8cd5),
            interfaceVersion: 6,
            blockNumber: 42329386
        });
        records[68] = RaindexDeployment({
            network: LibRainDeploy.BASE_SEPOLIA,
            deployedAddress: address(0xb05D73E6BCc26AEB5b67Ff68C6E9C6151073e3cE),
            interfaceVersion: 6,
            blockNumber: 42331638
        });
        records[69] = RaindexDeployment({
            network: LibRainDeploy.BASE_SEPOLIA,
            deployedAddress: address(0x4F4527919DA654C1718Ca6Ef194F6547226685f6),
            interfaceVersion: 6,
            blockNumber: 42404017
        });
        records[70] = RaindexDeployment({
            network: LibRainDeploy.BASE_SEPOLIA,
            deployedAddress: address(0xE0203890383Ca9C0c263f42a51C4C95211Cee6F9),
            interfaceVersion: 6,
            blockNumber: 42628606
        });
        records[71] = RaindexDeployment({
            network: LibRainDeploy.BASE_SEPOLIA,
            deployedAddress: address(0x025e93Be5c1780240C11E34B3387266754017D20),
            interfaceVersion: 6,
            blockNumber: 42658389
        });
        records[72] = RaindexDeployment({
            network: LibRainDeploy.BASE_SEPOLIA,
            deployedAddress: address(0x13b8f830e347a51778652C86cf3419b3E1Ab1205),
            interfaceVersion: 6,
            blockNumber: 42665138
        });
        records[73] = RaindexDeployment({
            network: LibRainDeploy.BASE_SEPOLIA,
            deployedAddress: address(0x86594ac4319230870C6E587f4ACa48FAb575619e),
            interfaceVersion: 6,
            blockNumber: 42788671
        });
        records[74] = RaindexDeployment({
            network: LibRainDeploy.BASE_SEPOLIA,
            deployedAddress: address(0x7992F7A3044258c9a200835970D5bFf275Efe5e7),
            interfaceVersion: 6,
            blockNumber: 42893619
        });
        records[75] = RaindexDeployment({
            network: LibRainDeploy.BASE_SEPOLIA,
            deployedAddress: address(0x1115EBC9C82F074454F6AC28f123B0684A453aF7),
            interfaceVersion: 6,
            blockNumber: 42895413
        });
        records[76] = RaindexDeployment({
            network: LibRainDeploy.BSC,
            deployedAddress: address(0xb1d6D10561D4e1792A7c6B336b0529e4bFb5Ea8F),
            interfaceVersion: 3,
            blockNumber: 37019233
        });
        records[77] = RaindexDeployment({
            network: LibRainDeploy.BSC,
            deployedAddress: address(0xd2938E7c9fe3597F78832CE780Feb61945c377d7),
            interfaceVersion: 4,
            blockNumber: 42510318
        });
        records[78] = RaindexDeployment({
            network: LibRainDeploy.BSC,
            deployedAddress: address(0xbc609623F5020f6Fc7481024862cD5EE3FFf52D7),
            interfaceVersion: 4,
            blockNumber: 42711830
        });
        records[79] = RaindexDeployment({
            network: LibRainDeploy.ETHEREUM,
            deployedAddress: address(0xf1224A483ad7F1E9aA46A8CE41229F32d7549A74),
            interfaceVersion: 3,
            blockNumber: 19165935
        });
        records[80] = RaindexDeployment({
            network: LibRainDeploy.ETHEREUM,
            deployedAddress: address(0x0eA6d458488d1cf51695e1D6e4744e6FB715d37C),
            interfaceVersion: 4,
            blockNumber: 21041924
        });
        records[81] = RaindexDeployment({
            network: LibRainDeploy.FLARE,
            deployedAddress: address(0xc95A5f8eFe14d7a20BD2E5BAFEC4E71f8Ce0B9A6),
            interfaceVersion: 3,
            blockNumber: 18708632
        });
        records[82] = RaindexDeployment({
            network: LibRainDeploy.FLARE,
            deployedAddress: address(0xF9bDedb1e8c32185E879E056eBA9F5aeC1839d60),
            interfaceVersion: 3,
            blockNumber: 21858702
        });
        records[83] = RaindexDeployment({
            network: LibRainDeploy.FLARE,
            deployedAddress: address(0xb06202aA3Fe7d85171fB7aA5f17011d17E63f382),
            interfaceVersion: 3,
            blockNumber: 22109423
        });
        records[84] = RaindexDeployment({
            network: LibRainDeploy.FLARE,
            deployedAddress: address(0x762adD85a30A83722feF2e029087C9D110B6a7b3),
            interfaceVersion: 3,
            blockNumber: 22412710
        });
        records[85] = RaindexDeployment({
            network: LibRainDeploy.FLARE,
            deployedAddress: address(0xD0790C27F9A8C163e55E2402c08C6ae2c53a2669),
            interfaceVersion: 3,
            blockNumber: 23429670
        });
        records[86] = RaindexDeployment({
            network: LibRainDeploy.FLARE,
            deployedAddress: address(0x07701e3BcE4248EFDFc7D31392a43c8b82a7A260),
            interfaceVersion: 3,
            blockNumber: 24243780
        });
        records[87] = RaindexDeployment({
            network: LibRainDeploy.FLARE,
            deployedAddress: address(0x4CebD993B31E8A17860337B2703f123A561F9685),
            interfaceVersion: 4,
            blockNumber: 26540405
        });
        records[88] = RaindexDeployment({
            network: LibRainDeploy.FLARE,
            deployedAddress: address(0x582d9e838FE6cD9F8147C66A8f56A3FBE513a6A2),
            interfaceVersion: 4,
            blockNumber: 26728124
        });
        records[89] = RaindexDeployment({
            network: LibRainDeploy.FLARE,
            deployedAddress: address(0xA2Ac77b982A9c0999472c1De378A81d7363d926F),
            interfaceVersion: 4,
            blockNumber: 28576185
        });
        records[90] = RaindexDeployment({
            network: LibRainDeploy.FLARE,
            deployedAddress: address(0xaa3b14Af0e29E3854E4148f43321C4410db002bC),
            interfaceVersion: 4,
            blockNumber: 28586117
        });
        records[91] = RaindexDeployment({
            network: LibRainDeploy.FLARE,
            deployedAddress: address(0xCEe8Cd002F151A536394E564b84076c41bBBcD4d),
            interfaceVersion: 4,
            blockNumber: 29307299
        });
        records[92] = RaindexDeployment({
            network: LibRainDeploy.FLARE,
            deployedAddress: address(0xe522cB4a5fCb2eb31a52Ff41a4653d85A4fd7C9D),
            interfaceVersion: 4,
            blockNumber: 42523429
        });
        records[93] = RaindexDeployment({
            network: LibRainDeploy.FLARE,
            deployedAddress: address(0x8Dbc50485e7C6994923B9A2c603a2B6b2d8Eef64),
            interfaceVersion: 5,
            blockNumber: 49782578
        });
        records[94] = RaindexDeployment({
            network: LibRainDeploy.FLARE,
            deployedAddress: address(0x8542c952c8D3441b9B861Fe582E2BC620058348E),
            interfaceVersion: 5,
            blockNumber: 50190133
        });
        records[95] = RaindexDeployment({
            network: LibRainDeploy.FLARE,
            deployedAddress: address(0x7242ce1ffe34297921c54cD7f9343c411E1E7759),
            interfaceVersion: 6,
            blockNumber: 56972130
        });
        records[96] = RaindexDeployment({
            network: LibRainDeploy.FLARE,
            deployedAddress: address(0xa560e58C77B8A841f80B4741Ad08E9260Edcab9F),
            interfaceVersion: 6,
            blockNumber: 61847874
        });
        records[97] = RaindexDeployment({
            network: LibRainDeploy.FLARE,
            deployedAddress: address(0x83507d7E14556A1822E6959982BebA6024245FA8),
            interfaceVersion: 6,
            blockNumber: 62094702
        });
        records[98] = RaindexDeployment({
            network: LibRainDeploy.FLARE,
            deployedAddress: address(0x3Ee54EbB1734DA14852C06A3Be1CdEebA796B0a3),
            interfaceVersion: 6,
            blockNumber: 62113357
        });
        records[99] = RaindexDeployment({
            network: LibRainDeploy.FLARE,
            deployedAddress: address(0xE8421069D1F56cC1207E6Ec21341BcFD044B8cd5),
            interfaceVersion: 6,
            blockNumber: 62117454
        });
        records[100] = RaindexDeployment({
            network: LibRainDeploy.FLARE,
            deployedAddress: address(0xb05D73E6BCc26AEB5b67Ff68C6E9C6151073e3cE),
            interfaceVersion: 6,
            blockNumber: 62120965
        });
        records[101] = RaindexDeployment({
            network: LibRainDeploy.FLARE,
            deployedAddress: address(0x4F4527919DA654C1718Ca6Ef194F6547226685f6),
            interfaceVersion: 6,
            blockNumber: 62235114
        });
        records[102] = RaindexDeployment({
            network: LibRainDeploy.FLARE,
            deployedAddress: address(0xE0203890383Ca9C0c263f42a51C4C95211Cee6F9),
            interfaceVersion: 6,
            blockNumber: 62584927
        });
        records[103] = RaindexDeployment({
            network: LibRainDeploy.FLARE,
            deployedAddress: address(0x025e93Be5c1780240C11E34B3387266754017D20),
            interfaceVersion: 6,
            blockNumber: 62629754
        });
        records[104] = RaindexDeployment({
            network: LibRainDeploy.FLARE,
            deployedAddress: address(0x13b8f830e347a51778652C86cf3419b3E1Ab1205),
            interfaceVersion: 6,
            blockNumber: 62642618
        });
        records[105] = RaindexDeployment({
            network: LibRainDeploy.FLARE,
            deployedAddress: address(0x86594ac4319230870C6E587f4ACa48FAb575619e),
            interfaceVersion: 6,
            blockNumber: 62835540
        });
        records[106] = RaindexDeployment({
            network: LibRainDeploy.FLARE,
            deployedAddress: address(0x7992F7A3044258c9a200835970D5bFf275Efe5e7),
            interfaceVersion: 6,
            blockNumber: 63004971
        });
        records[107] = RaindexDeployment({
            network: LibRainDeploy.FLARE,
            deployedAddress: address(0x1115EBC9C82F074454F6AC28f123B0684A453aF7),
            interfaceVersion: 6,
            blockNumber: 63007250
        });
        records[108] = RaindexDeployment({
            network: "linea",
            deployedAddress: address(0xF97DE1c2d864d90851aDBcbEe0A38260440B8D90),
            interfaceVersion: 4,
            blockNumber: 7439620
        });
        records[109] = RaindexDeployment({
            network: "linea",
            deployedAddress: address(0x22410e2a46261a1B1e3899a072f303022801C764),
            interfaceVersion: 4,
            blockNumber: 10147523
        });
        records[110] = RaindexDeployment({
            network: "matchain",
            deployedAddress: address(0x40312EDAB8Fe65091354172ad79e9459f21094E2),
            interfaceVersion: 4,
            blockNumber: 3976602
        });
        records[111] = RaindexDeployment({
            network: LibRainDeploy.POLYGON,
            deployedAddress: address(0xFb8a0C401C9d11fDecCdDDCBf89bFFA84681281d),
            interfaceVersion: 3,
            blockNumber: 47330901
        });
        records[112] = RaindexDeployment({
            network: LibRainDeploy.POLYGON,
            deployedAddress: address(0x95C9Bf235435b660AA69F519904C3F175Aab393d),
            interfaceVersion: 3,
            blockNumber: 49421712
        });
        records[113] = RaindexDeployment({
            network: LibRainDeploy.POLYGON,
            deployedAddress: address(0xd3eDaFeb9eAA454CE26E60A66CcDA73939C343A4),
            interfaceVersion: 3,
            blockNumber: 49464751
        });
        records[114] = RaindexDeployment({
            network: LibRainDeploy.POLYGON,
            deployedAddress: address(0xdcdee0E7a58Bba7e305dB3Abc42F4887CE8EF729),
            interfaceVersion: 3,
            blockNumber: 50715570
        });
        records[115] = RaindexDeployment({
            network: LibRainDeploy.POLYGON,
            deployedAddress: address(0x16d518706d666C549DA7Bd31110623B09eF23AbB),
            interfaceVersion: 3,
            blockNumber: 50903721
        });
        records[116] = RaindexDeployment({
            network: LibRainDeploy.POLYGON,
            deployedAddress: address(0xDE5aBE2837bc042397D80E37fb7b2C850a8d5a6C),
            interfaceVersion: 3,
            blockNumber: 52621167
        });
        records[117] = RaindexDeployment({
            network: LibRainDeploy.POLYGON,
            deployedAddress: address(0xc95A5f8eFe14d7a20BD2E5BAFEC4E71f8Ce0B9A6),
            interfaceVersion: 3,
            blockNumber: 54697866
        });
        records[118] = RaindexDeployment({
            network: LibRainDeploy.POLYGON,
            deployedAddress: address(0x5Cf7d0a8c61c8dcC6b0ECB281dF1C17264C2A517),
            interfaceVersion: 3,
            blockNumber: 55960481
        });
        records[119] = RaindexDeployment({
            network: LibRainDeploy.POLYGON,
            deployedAddress: address(0x5CeEe9F4F49C106D5Bc049C8D649C332E6d365ad),
            interfaceVersion: 3,
            blockNumber: 58063043
        });
        records[120] = RaindexDeployment({
            network: LibRainDeploy.POLYGON,
            deployedAddress: address(0xAfD94467d2eC43D9aD39f835BA758b61b2f41A0E),
            interfaceVersion: 4,
            blockNumber: 59714619
        });
        records[121] = RaindexDeployment({
            network: LibRainDeploy.POLYGON,
            deployedAddress: address(0x2f209e5b67A33B8fE96E28f24628dF6Da301c8eB),
            interfaceVersion: 4,
            blockNumber: 59720459
        });
        records[122] = RaindexDeployment({
            network: LibRainDeploy.POLYGON,
            deployedAddress: address(0x001B302095D66b777C04cd4d64b86CCe16de55A1),
            interfaceVersion: 4,
            blockNumber: 60620609
        });
        records[123] = RaindexDeployment({
            network: LibRainDeploy.POLYGON,
            deployedAddress: address(0xb8CD71e3b4339c8B718D982358cB32Ed272e4174),
            interfaceVersion: 4,
            blockNumber: 60623145
        });
        records[124] = RaindexDeployment({
            network: LibRainDeploy.POLYGON,
            deployedAddress: address(0x7D2f700b1f6FD75734824EA4578960747bdF269A),
            interfaceVersion: 4,
            blockNumber: 62036116
        });
        records[125] = RaindexDeployment({
            network: LibRainDeploy.POLYGON,
            deployedAddress: address(0x8a3C8E610d827093F7437E0C45EFa648563c0dDA),
            interfaceVersion: 5,
            blockNumber: 76772162
        });
        records[126] = RaindexDeployment({
            network: LibRainDeploy.POLYGON,
            deployedAddress: address(0x7242ce1ffe34297921c54cD7f9343c411E1E7759),
            interfaceVersion: 6,
            blockNumber: 84174550
        });
        records[127] = RaindexDeployment({
            network: LibRainDeploy.POLYGON,
            deployedAddress: address(0xa560e58C77B8A841f80B4741Ad08E9260Edcab9F),
            interfaceVersion: 6,
            blockNumber: 87623401
        });
        records[128] = RaindexDeployment({
            network: LibRainDeploy.POLYGON,
            deployedAddress: address(0x83507d7E14556A1822E6959982BebA6024245FA8),
            interfaceVersion: 6,
            blockNumber: 87804686
        });
        records[129] = RaindexDeployment({
            network: LibRainDeploy.POLYGON,
            deployedAddress: address(0x3Ee54EbB1734DA14852C06A3Be1CdEebA796B0a3),
            interfaceVersion: 6,
            blockNumber: 87818427
        });
        records[130] = RaindexDeployment({
            network: LibRainDeploy.POLYGON,
            deployedAddress: address(0xE8421069D1F56cC1207E6Ec21341BcFD044B8cd5),
            interfaceVersion: 6,
            blockNumber: 87821341
        });
        records[131] = RaindexDeployment({
            network: LibRainDeploy.POLYGON,
            deployedAddress: address(0xb05D73E6BCc26AEB5b67Ff68C6E9C6151073e3cE),
            interfaceVersion: 6,
            blockNumber: 87823915
        });
        records[132] = RaindexDeployment({
            network: LibRainDeploy.POLYGON,
            deployedAddress: address(0x4F4527919DA654C1718Ca6Ef194F6547226685f6),
            interfaceVersion: 6,
            blockNumber: 87914243
        });
        records[133] = RaindexDeployment({
            network: LibRainDeploy.POLYGON,
            deployedAddress: address(0xE0203890383Ca9C0c263f42a51C4C95211Cee6F9),
            interfaceVersion: 6,
            blockNumber: 88213689
        });
        records[134] = RaindexDeployment({
            network: LibRainDeploy.POLYGON,
            deployedAddress: address(0x025e93Be5c1780240C11E34B3387266754017D20),
            interfaceVersion: 6,
            blockNumber: 88253394
        });
        records[135] = RaindexDeployment({
            network: LibRainDeploy.POLYGON,
            deployedAddress: address(0x13b8f830e347a51778652C86cf3419b3E1Ab1205),
            interfaceVersion: 6,
            blockNumber: 88264952
        });
        records[136] = RaindexDeployment({
            network: LibRainDeploy.POLYGON,
            deployedAddress: address(0x86594ac4319230870C6E587f4ACa48FAb575619e),
            interfaceVersion: 6,
            blockNumber: 88427102
        });
        records[137] = RaindexDeployment({
            network: LibRainDeploy.POLYGON,
            deployedAddress: address(0x7992F7A3044258c9a200835970D5bFf275Efe5e7),
            interfaceVersion: 6,
            blockNumber: 88567034
        });
        records[138] = RaindexDeployment({
            network: LibRainDeploy.POLYGON,
            deployedAddress: address(0x1115EBC9C82F074454F6AC28f123B0684A453aF7),
            interfaceVersion: 6,
            blockNumber: 88569424
        });
        return records;
    }
}
