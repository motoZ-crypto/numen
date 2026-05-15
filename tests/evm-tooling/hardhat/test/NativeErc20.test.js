// Native UNIT as ERC20 (precompile at 0x0000…0802) round-trip.
//
//   npx hardhat --network cryptoNode test mocha
//
// This complements `Token.test.js` by verifying that an EVM-side caller can
// drive the native balance pallet through the standard ERC20 ABI exposed at
// the precompile address — which is the path that "transfers funds back"
// from contracts/wallets that only know how to talk ERC20.

import { expect } from "chai";
import { network } from "hardhat";

const { ethers } = await network.create("cryptoNode");

const PRECOMPILE = "0x0000000000000000000000000000000000000802";

const ERC20_ABI = [
  "function name() view returns (string)",
  "function symbol() view returns (string)",
  "function decimals() view returns (uint8)",
  "function balanceOf(address) view returns (uint256)",
  "function transfer(address,uint256) returns (bool)",
];

describe("Native UNIT ERC20 precompile (0x0802)", function () {
  this.timeout(120_000);

  it("exposes UNIT metadata", async function () {
    const erc20 = new ethers.Contract(PRECOMPILE, ERC20_ABI, ethers.provider);
    expect(await erc20.name()).to.equal("UNIT");
    expect(await erc20.symbol()).to.equal("UNIT");
    expect(Number(await erc20.decimals())).to.equal(18);
  });

  it("balanceOf agrees with eth_getBalance", async function () {
    const [alith] = await ethers.getSigners();
    const erc20 = new ethers.Contract(PRECOMPILE, ERC20_ABI, ethers.provider);

    const erc20Bal = await erc20.balanceOf(alith.address);
    const nativeBal = await ethers.provider.getBalance(alith.address);
    expect(erc20Bal).to.equal(nativeBal);
  });

  it("transfer moves native UNIT between mirror accounts", async function () {
    const [alith, baltathar] = await ethers.getSigners();
    const erc20 = new ethers.Contract(PRECOMPILE, ERC20_ABI, alith);

    const amount = ethers.parseEther("3.0");
    const beforeTo = await erc20.balanceOf(baltathar.address);

    const tx = await erc20.transfer(baltathar.address, amount);
    const receipt = await tx.wait();
    expect(receipt.status).to.equal(1);

    const afterTo = await erc20.balanceOf(baltathar.address);
    expect(afterTo - beforeTo).to.equal(amount);

    // ERC20 view and native getBalance must remain consistent post-transfer.
    const afterToNative = await ethers.provider.getBalance(baltathar.address);
    expect(afterToNative).to.equal(afterTo);
  });
});
