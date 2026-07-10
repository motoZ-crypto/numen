// Token round-trip test against the live cryptoNode dev chain.
//
//   npx hardhat --network cryptoNode test mocha

import { expect } from "chai";
import { network } from "hardhat";

const { ethers } = await network.create("cryptoNode");

describe("Token (cryptoNode)", function () {
  // Real chain calls take seconds, not milliseconds — relax the default
  // mocha timeout so block-confirmation latency doesn't fail the suite.
  this.timeout(120_000);

  it("reports the configured EVM chain id (320262)", async function () {
    const { chainId } = await ethers.provider.getNetwork();
    expect(Number(chainId)).to.equal(320262);
  });

  it("deploys, transfers, and reflects the new balances", async function () {
    const [alith, baltathar] = await ethers.getSigners();
    const supply = ethers.parseUnits("1000", 18);

    const Token = await ethers.getContractFactory("Token", alith);
    const token = await Token.deploy(supply);
    await token.waitForDeployment();

    expect(await token.totalSupply()).to.equal(supply);
    expect(await token.balanceOf(alith.address)).to.equal(supply);

    const amount = ethers.parseUnits("100", 18);
    const tx = await token.transfer(baltathar.address, amount);
    await tx.wait();

    expect(await token.balanceOf(alith.address)).to.equal(supply - amount);
    expect(await token.balanceOf(baltathar.address)).to.equal(amount);
  });
});
