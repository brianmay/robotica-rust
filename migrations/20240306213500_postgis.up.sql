CREATE EXTENSION postgis;

CREATE TABLE locations (
    id SERIAL PRIMARY KEY,
    name VARCHAR(64) NOT NULL,
    bounds geography(POLYGON,4326) NOT NULL
  );

CREATE INDEX locations_gix ON locations USING GIST ( bounds );
