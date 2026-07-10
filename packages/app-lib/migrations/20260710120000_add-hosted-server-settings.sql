-- Player-facing settings a server owner can edit after creation. Both are the
-- source of truth (enforced into server.properties on every start, and
-- max_players is also mirrored into Gate's status ping), so they need to be
-- stored rather than read back out of server.properties. Defaults match what
-- default_server_properties has always written, so existing rows are unchanged.
ALTER TABLE hosted_servers
    ADD COLUMN max_players INTEGER NOT NULL DEFAULT 20;
ALTER TABLE hosted_servers
    ADD COLUMN view_distance INTEGER NOT NULL DEFAULT 8;
