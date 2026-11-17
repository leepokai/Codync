-- The app version each device last reported (`Codync-App: <platform>/<version>; minHost=<version>`), so a
-- cloud computer can move to the host its owner's apps need (docs/plans/codync-cloud.md, cloud soft update).
ALTER TABLE devices ADD COLUMN app_version TEXT;
ALTER TABLE devices ADD COLUMN min_host TEXT;
ALTER TABLE devices ADD COLUMN app_seen_at INTEGER;

-- Published host releases and the oldest app each one works with (its MIN_APP), read from GitHub by the cron.
-- Only releases up to the one with GitHub's latest mark are kept.
CREATE TABLE host_releases (
  version TEXT PRIMARY KEY,
  min_app TEXT NOT NULL
);
