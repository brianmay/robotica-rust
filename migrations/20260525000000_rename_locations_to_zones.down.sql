ALTER TABLE zones RENAME TO locations;
ALTER INDEX zones_pkey RENAME TO locations_pkey;
ALTER INDEX zones_gix RENAME TO locations_gix;
