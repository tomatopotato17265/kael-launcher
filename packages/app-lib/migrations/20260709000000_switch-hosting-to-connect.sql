-- 20260526120000_fix-skin-selector-identity.sql dropped default_minecraft_capes
-- but left this trigger behind, since it lives on minecraft_users rather than
-- on the table it was dropped with. SQLite only re-validates every trigger in
-- the schema on a DROP COLUMN/RENAME COLUMN, so this dangling reference to a
-- nonexistent table has silently blocked any such migration since; it must go
-- before this migration's own DROP/RENAME COLUMN statements below can run.
DROP TRIGGER IF EXISTS default_minecraft_capes_user_uuid_update_cascade;

-- Hosting moved from playit.gg + Cloudflare DNS to Minekube Connect: each
-- server now links its own Gate process to Connect instead of sharing one
-- playit agent, so the pid column tracks a per-server process and the tunnel
-- address is a stable Connect endpoint name rather than a rotating playit
-- tunnel plus a Cloudflare-managed custom domain.
ALTER TABLE hosted_servers DROP COLUMN playit_tunnel_id;
ALTER TABLE hosted_servers DROP COLUMN tunnel_url;
ALTER TABLE hosted_servers DROP COLUMN custom_domain;
ALTER TABLE hosted_servers DROP COLUMN cf_record_ids;
ALTER TABLE hosted_servers ADD COLUMN endpoint_name TEXT NULL;
ALTER TABLE hosted_servers RENAME COLUMN agent_pid TO gate_pid;

DROP TABLE playit_account;
DROP TABLE dns_owner;
