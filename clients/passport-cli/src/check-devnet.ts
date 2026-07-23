import { Connection, clusterApiUrl } from "@solana/web3.js";
import { DEVNET_GENESIS_HASH } from "./core.js";

const connection = new Connection(clusterApiUrl("devnet"), "confirmed");
const [genesisHash, version] = await Promise.all([
  connection.getGenesisHash(),
  connection.getVersion(),
]);
if (genesisHash !== DEVNET_GENESIS_HASH) {
  throw new Error(`unexpected Devnet genesis hash: ${genesisHash}`);
}
process.stdout.write(`${JSON.stringify({ cluster: "devnet", genesisHash, version }, null, 2)}\n`);
