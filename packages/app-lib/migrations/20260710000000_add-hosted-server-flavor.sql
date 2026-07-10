-- Hosted servers now run Paper rather than vanilla, so Gate can hand the
-- backend the real Mojang profile over Velocity forwarding. Vanilla speaks no
-- forwarding protocol at all, so it can only ever see offline UUIDs.
--
-- Servers created before this change keep running vanilla behind a Lite-mode
-- Gate: rendering velocity forwarding at a vanilla backend would break them
-- outright. The flavor decides which Gate config shape is rendered, so it must
-- default to 'vanilla' for every existing row.
ALTER TABLE hosted_servers
    ADD COLUMN flavor TEXT NOT NULL DEFAULT 'vanilla';
