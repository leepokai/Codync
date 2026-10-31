import { applyD1Migrations } from "cloudflare:test";
import { env as rawEnv } from "cloudflare:workers";
import { describe, expect, it } from "vitest";
import { call, clerkToken, env } from "./helpers";
import * as ref from "./ref";

describe("Android devices", () => {
  it("registers a signed Android client and preserves revocation", async () => {
    const userId = "user_android_" + ref.b64url(ref.random(8));
    const token = await clerkToken(userId);
    const key = ref.signKey();
    const registered = await call("POST", "/v1/devices", { token, key, body: { name: "Samsung", platform: "android" } });
    expect(registered.status).toBe(200);
    expect(registered.body.device.platform).toBe("android");
    const listed = await call("GET", "/v1/devices", { token });
    expect(listed.body.devices[0].platform).toBe("android");
    const revoked = await call("DELETE", "/v1/devices/" + registered.body.device.deviceId, { token });
    expect(revoked.status).toBe(200);
    const refused = await call("POST", "/v1/devices", { token, key, body: { name: "Samsung", platform: "android" } });
    expect(refused.status).toBe(403);
  });

  it("adds Android after the existing desktop migration without losing authorizations or constraints", async () => {
    const db = (rawEnv as unknown as { ANDROID_MIGRATION_DB: D1Database }).ANDROID_MIGRATION_DB;
    const androidMigration = env.TEST_MIGRATIONS.findIndex(migration => migration.name === "0004_android_devices.sql");
    expect(androidMigration).toBeGreaterThan(0);
    await applyD1Migrations(db, env.TEST_MIGRATIONS.slice(0, androidMigration));
    await db.batch([
      db.prepare("INSERT INTO accounts(user_id,created_at) VALUES ('android-owner',1)"),
      db.prepare("INSERT INTO computers(id,sign_pub,box_pub,owner_user_id,name,platform,version,created_at,updated_at) VALUES ('host','sign','box','android-owner','Host','linux','2.4.0',1,1)"),
      ...["ios", "linux", "macos", "windows"].map(platform => db.prepare("INSERT INTO devices(id,owner_user_id,sign_pub,name,platform,created_at,last_used_at,revoked_at) VALUES (?, 'android-owner', ?, ?, ?, 1, 2, 3)").bind(platform, platform + "-key", platform, platform)),
      db.prepare("INSERT INTO grants(id,computer_id,device_id,scopes,status,created_at) VALUES ('grant','host','ios','[\"control\"]','revoked',1)"),
      db.prepare("INSERT INTO access_requests(id,computer_id,device_id,user_id,status,commit_hash,created_at,expires_at) VALUES ('request','host','windows','android-owner','denied','hash',1,2)"),
    ]);
    const tables = ["devices", "grants", "access_requests"];
    const before = await Promise.all(tables.map(table => db.prepare(`SELECT * FROM ${table} ORDER BY id`).all()));
    // Only the Android migration: later ones add columns of their own.
    await applyD1Migrations(db, env.TEST_MIGRATIONS.slice(androidMigration, androidMigration + 1));
    const after = await Promise.all(tables.map(table => db.prepare(`SELECT * FROM ${table} ORDER BY id`).all()));
    expect(after.map(result => result.results)).toEqual(before.map(result => result.results));
    const old = await db.prepare("SELECT platform,created_at,last_used_at,revoked_at FROM devices ORDER BY platform").all();
    expect(old.results).toEqual(["ios", "linux", "macos", "windows"].map(platform => ({ platform, created_at: 1, last_used_at: 2, revoked_at: 3 })));
    const grant = await db.prepare("SELECT device_id FROM grants").first();
    const request = await db.prepare("SELECT device_id FROM access_requests").first();
    expect(grant).toEqual({ device_id: "ios" });
    expect(request).toEqual({ device_id: "windows" });
    const foreignKeys = await db.prepare("PRAGMA foreign_key_check").all();
    expect(foreignKeys.results).toEqual([]);
    await db.prepare("INSERT INTO devices(id,owner_user_id,sign_pub,name,platform,created_at) VALUES ('android','android-owner','android-key','Samsung','android',1)").run();
    await expect(db.prepare("INSERT INTO devices(id,owner_user_id,sign_pub,name,platform,created_at) VALUES ('duplicate','android-owner','android-key','Duplicate','android',1)").run()).rejects.toThrow();
    await expect(db.prepare("INSERT INTO devices(id,owner_user_id,sign_pub,name,platform,created_at) VALUES ('other','missing','other-key','Other','android',1)").run()).rejects.toThrow();
  });
});
