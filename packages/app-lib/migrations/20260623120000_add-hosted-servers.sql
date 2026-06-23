CREATE TABLE hosted_servers (
    id TEXT NOT NULL,
    name TEXT NOT NULL,
    directory TEXT NOT NULL,
    mc_version TEXT NOT NULL,
    java_path TEXT NULL,
    port INTEGER NOT NULL DEFAULT 25565,
    playit_tunnel_id TEXT NULL,
    tunnel_url TEXT NULL,
    created INTEGER NOT NULL,
    modified INTEGER NOT NULL,

    PRIMARY KEY (id)
);

CREATE TABLE playit_account (
    id INTEGER NOT NULL CHECK (id = 0),
    secret_key TEXT NOT NULL,
    account_type TEXT NOT NULL,

    PRIMARY KEY (id)
);
