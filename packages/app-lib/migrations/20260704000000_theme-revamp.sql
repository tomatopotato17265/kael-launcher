ALTER TABLE settings ADD COLUMN color_theme TEXT NOT NULL DEFAULT '#000000';
ALTER TABLE settings ADD COLUMN dark_color_theme TEXT NOT NULL DEFAULT '#000000';
ALTER TABLE settings ADD COLUMN dark_brand_color TEXT NOT NULL DEFAULT '#874EFE';
ALTER TABLE settings ADD COLUMN sync_theme_with_system INTEGER NOT NULL DEFAULT FALSE;
ALTER TABLE settings ADD COLUMN active_theme_preset TEXT NULL;
ALTER TABLE settings ADD COLUMN dark_active_theme_preset TEXT NULL;
ALTER TABLE settings ADD COLUMN theme_dir TEXT NULL;

-- brand_color already exists nullable with no default (20260623130000_add-brand-color.sql);
-- backfill so it can be treated as non-optional going forward.
UPDATE settings SET brand_color = '#874EFE' WHERE brand_color IS NULL;
