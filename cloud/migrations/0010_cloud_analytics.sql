-- Whether a cloud computer's own host shares usage analytics, as it last said (its owner's switch, sent when it changes
-- and with each sleep request): the Worker reports that machine's wakes to PostHog only while it is 1
-- (src/analytics.ts). NULL until its host first says; NULL or 0, the Worker sends nothing about it.
ALTER TABLE cloud_machines ADD COLUMN analytics INTEGER;
