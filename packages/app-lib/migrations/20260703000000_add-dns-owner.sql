CREATE TABLE dns_owner (
    id INTEGER PRIMARY KEY CHECK (id = 0),
    owner_key TEXT NOT NULL
);
