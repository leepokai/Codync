-- Historical migration, already deployed. Ordering is now device-local; this column is unused.
ALTER TABLE accounts ADD COLUMN computer_order TEXT;
