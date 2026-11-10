// A cloud computer's keeper (docs/plans/codync-cloud.md, "Agents and the host's secrets"): a process on the machine,
// under its own system user, that holds the machine's secrets so its agents (another user, without sudo) can use them
// but never read them. Its vault is sealed with a key only this Worker keeps (`cloud_machines.vault_sealed`, made at
// create): the keeper fetches it at each start and keeps it in memory, so the disk holds ciphertext. The claim names the
// keeper's Ed25519 key once (`hostClaim`), and only a request that key signed gets the vault key. That key is on the
// disk too, so a copy of the disk (a snapshot, a backup) can fetch the vault key until the machine is deleted, which
// erases it.

import { machineByKeeper, vaultKeyOf } from "../cloud-machines";
import { ApiError, type Ctx, signed } from "./common";

/** `GET /v1/keeper/vault-key` (Sig(keeper)): `{key}`, the machine's vault key (32 bytes, base64url). 404 until a
 * claim named this keeper, and once its machine is deleted. */
export async function keeperVaultKey(c: Ctx) {
  const s = await signed(c);
  const row = await machineByKeeper(c.env, s.kid);
  if (!row?.vault_sealed || !row.vault_key_id) throw new ApiError("notFound");
  const key = await vaultKeyOf(c.env, row);
  if (!key) {
    console.error(JSON.stringify({ msg: "cloud computer vault key can't be opened", machine: row.id }));
    throw new ApiError("internal");
  }
  return { key };
}
