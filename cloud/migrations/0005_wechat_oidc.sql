-- A WeChat sign-in in progress. The browser only carries a random state; Clerk's
-- redirect, nonce and the profile stay here until the code is used or it expires.
-- expires_at is that authorization deadline. access_expires_at is the access
-- token's own deadline, set when the token is issued.
CREATE TABLE wechat_oidc (
  state             TEXT PRIMARY KEY,
  client_id         TEXT NOT NULL,
  redirect_uri      TEXT NOT NULL,
  clerk_state       TEXT NOT NULL,
  nonce             TEXT NOT NULL,
  code_challenge    TEXT,
  auth_code         TEXT,
  access_token      TEXT,
  subject           TEXT,
  name              TEXT,
  picture           TEXT,
  expires_at        INTEGER NOT NULL,
  access_expires_at INTEGER,
  consumed_at       INTEGER
);
CREATE UNIQUE INDEX wechat_oidc_auth_code ON wechat_oidc(auth_code);
CREATE UNIQUE INDEX wechat_oidc_access_token ON wechat_oidc(access_token);

-- Subject already issued for a WeChat openid. The first one wins, so a later
-- profile failure cannot mint a second Clerk account for the same person.
CREATE TABLE wechat_subject (
  openid  TEXT PRIMARY KEY,
  subject TEXT NOT NULL
);
