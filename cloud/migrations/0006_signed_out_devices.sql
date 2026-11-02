-- A phone that signed out keeps its key and its access, but its notifications stop: the account's computers drop
-- its push tickets on their next state pull (`signedOut` in GET /v1/host/state). Registering it again clears this.
ALTER TABLE devices ADD COLUMN signed_out_at INTEGER;
