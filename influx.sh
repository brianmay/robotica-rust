#!/usr/bin/env sh
influx setup --skip-verify --force \
  --bucket "sensors" \
  --org "robotica" \
  --username "admin" \
  --password "passpass" \
  --token "token"

