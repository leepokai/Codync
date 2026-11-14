-- A phone that signed out keeps its key and its access, but its notifications stop: the account's computers drop
-- the push tickets it registered up to then on their next state pull (`signedOutDevices` in GET /v1/host/state, with
-- `signed_out_at`). Registering it again clears this.
ALTER TABLE devices ADD COLUMN signed_out_at INTEGER;
