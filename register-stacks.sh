#!/bin/bash

# Rehearsa — bulk stack registration
# Registers all production stacks with a nightly 3am schedule

SCHEDULE="0 3 * * *"

STACKS=(
    "arr-stack /mnt/nvme/docker/stacks/arr-stack/docker-compose.yml"
    "dashy /mnt/nvme/docker/stacks/dashy/docker-compose.yml"
    "edge /mnt/nvme/docker/stacks/edge/docker-compose.yml"
    "filebrowser /mnt/nvme/docker/stacks/filebrowser/docker-compose.yml"
    "glances /mnt/nvme/docker/stacks/glances/docker-compose.yml"
    "gluetun /mnt/nvme/docker/stacks/gluetun/docker-compose.yml"
    "jellyfin /mnt/nvme/docker/stacks/jellyfin/docker-compose.yml"
    "linkwarden /mnt/nvme/docker/stacks/linkwarden/docker-compose.yml"
    "matrix /mnt/nvme/docker/stacks/matrix/docker-compose.yml"
    "navidrome /mnt/nvme/docker/stacks/navidrome/docker-compose.yml"
    "notes /mnt/nvme/docker/stacks/notes/docker-compose.yml"
    "ollama /mnt/nvme/docker/stacks/ollama/docker-compose.yml"
    "paperless /mnt/nvme/docker/stacks/paperless/docker-compose.yml"
    "radarr-4k /mnt/nvme/docker/stacks/radarr-4k/docker-compose.yml"
    "restic /mnt/nvme/docker/stacks/restic/docker-compose.yml"
    "speedtest /mnt/nvme/docker/stacks/speedtest/docker-compose.yml"
    "syncthing /mnt/nvme/docker/stacks/syncthing/docker-compose.yml"
    "uptime /mnt/nvme/docker/stacks/uptime/docker-compose.yml"
    "vaultwarden /mnt/nvme/docker/stacks/vaultwarden/docker-compose.yml"
    "watchtower /mnt/nvme/docker/stacks/watchtower/docker-compose.yml"
    "wizarr /mnt/nvme/docker/stacks/wizarr/docker-compose.yml"
    "pagerr /mnt/nvme/docker/pagerr/docker-compose.yml"
    "smartarr /mnt/nvme/docker/smartarr/docker-compose.yml"
    "pihole /mnt/nvme/docker/pihole/docker-compose.yml"
)

echo "Registering ${#STACKS[@]} stacks with Rehearsa..."
echo

for entry in "${STACKS[@]}"; do
    name=$(echo "$entry" | cut -d' ' -f1)
    path=$(echo "$entry" | cut -d' ' -f2)

    if [ ! -f "$path" ]; then
        echo "  SKIP $name — file not found: $path"
        continue
    fi

    rehearsa daemon watch "$name" "$path" --schedule "$SCHEDULE" 2>/dev/null
    echo "  OK   $name"
done

echo
echo "Done. Run 'rehearsa daemon list' to verify."
echo "Then run 'sudo rehearsa baseline auto-init' to rehearse and contract all stacks."
