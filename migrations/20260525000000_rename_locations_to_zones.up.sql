ALTER TABLE locations RENAME TO zones;
ALTER INDEX locations_pkey RENAME TO zones_pkey;
ALTER INDEX locations_gix RENAME TO zones_gix;
