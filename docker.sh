#!/bin/sh
set -ex
docker run --rm --ti \
    -p 4000:4000 \
    -v /home/brian/tree/personal/relixir/local/config:/config \
    --env HOSTNAME="$HOSTNAME" \
    --env ROOT_URL="$ROOT_URL" \
    --env MQTT_HOST="$MQTT_HOST" \
    --env MQTT_PORT="$MQTT_PORT" \
    --env MQTT_USERNAME="$MQTT_USERNAME" \
    --env MQTT_PASSWORD="$MQTT_PASSWORD" \
    --env LIFE360_USERNAME="$LIFE360_USERNAME" \
    --env LIFE360_PASSWORD="$LIFE360_PASSWORD" \
    --env CLASSIFICATIONS_FILE="/config/classifications.yaml" \
    --env SCHEDULE_FILE="/config/schedule.yaml" \
    --env SEQUENCES_FILE="/config/sequences.yaml" \
    --env SESSION_SECRET="$SESSION_SECRET" \
    --env OIDC_DISCOVERY_URL="$OIDC_DISCOVERY_URL" \
    --env OIDC_CLIENT_ID="$OIDC_CLIENT_ID" \
    --env OIDC_CLIENT_SECRET="$OIDC_CLIENT_SECRET" \
    --env OIDC_AUTH_SCOPE="$OIDC_AUTH_SCOPE" \
    robotica-node-rust:latest \
    "$@"
