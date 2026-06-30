ALTER TABLE minecraft_users ADD COLUMN logged_in INTEGER NOT NULL DEFAULT 0;
UPDATE minecraft_users SET logged_in = strftime('%s', 'now');
