-- The one Azure connection an administrator saves from Settings. It lives in
-- its own table, not in `secrets`, so the client secret is never a vault
-- entry a run could resolve into its environment.
CREATE TABLE azure_connection (
    id INTEGER PRIMARY KEY NOT NULL,
    config TEXT NOT NULL,
    client_secret TEXT NOT NULL,
    secret_updated_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    CHECK (id = 1),
    CHECK (json_valid(config)),
    CHECK (length(client_secret) > 0)
);
