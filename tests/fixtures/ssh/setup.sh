#!/usr/bin/env bash
# Un bastión SSH desechable en la red Docker de kind, para probar el túnel (D13).
#
#   tests/fixtures/ssh/setup.sh [directorio-de-salida]
#
# Necesita el clúster de tests/fixtures/kind/setup.sh. Todo lo de SSH (clave,
# known_hosts, ssh_config) se genera en el directorio de salida: nunca se lee ni
# se toca ~/.ssh ni el agente del usuario.
set -euo pipefail

KIND_NAME="${OCULUS_KIND_NAME:-oculus-it}"
NAME="${OCULUS_SSHD_NAME:-oculus-it-sshd}"
PORT="${OCULUS_SSHD_PORT:-2222}"
OUT="${1:-${TMPDIR:-/tmp}/oculus-it-ssh}"
mkdir -p "$OUT"
OUT="$(cd "$OUT" && pwd)"

docker build -q -t "$NAME" - > /dev/null <<'DOCKERFILE'
FROM alpine:3.22
RUN apk add --no-cache openssh-server \
 && ssh-keygen -A \
 && adduser -D -s /bin/sh tunnel \
 # Cuenta sin contraseña pero no bloqueada: sshd rechaza las bloqueadas ("!").
 && sed -i 's/^tunnel:!/tunnel:*/' /etc/shadow \
 && mkdir -p /home/tunnel/.ssh && chmod 700 /home/tunnel/.ssh && chown tunnel:tunnel /home/tunnel/.ssh \
 && printf '%s\n' 'PasswordAuthentication no' 'KbdInteractiveAuthentication no' 'PermitRootLogin no' \
      'AllowTcpForwarding yes' 'AllowUsers tunnel' > /etc/ssh/sshd_config.d/oculus.conf
EXPOSE 22
CMD ["/usr/sbin/sshd", "-D", "-e"]
DOCKERFILE

docker rm -f "$NAME" > /dev/null 2>&1 || true
docker run -d --name "$NAME" --network kind -p "127.0.0.1:$PORT:22" "$NAME" > /dev/null

rm -f "$OUT/id_ed25519" "$OUT/id_ed25519.pub"
ssh-keygen -q -t ed25519 -N "" -C oculus-it -f "$OUT/id_ed25519"
docker exec -i "$NAME" sh -c \
  'cat > /home/tunnel/.ssh/authorized_keys && chmod 600 /home/tunnel/.ssh/authorized_keys && chown tunnel:tunnel /home/tunnel/.ssh/authorized_keys' \
  < "$OUT/id_ed25519.pub"
echo "[127.0.0.1]:$PORT $(docker exec "$NAME" cat /etc/ssh/ssh_host_ed25519_key.pub | cut -d' ' -f1,2)" > "$OUT/known_hosts"

cat > "$OUT/ssh_config" <<CONFIG
# Solo para los tests de Oculus. Se usa con ssh -F.
Host oculus-it-bastion oculus-it-sinclave
  HostName 127.0.0.1
  Port $PORT
  UserKnownHostsFile $OUT/known_hosts
  GlobalKnownHostsFile /dev/null
  StrictHostKeyChecking yes
  IdentityAgent none
  IdentitiesOnly yes
Host oculus-it-bastion
  User tunnel
  IdentityFile $OUT/id_ed25519
Host oculus-it-sinclave
  User nadie
  IdentityFile /dev/null
CONFIG

# Kubeconfig con la dirección interna del API server (oculus-it-control-plane:6443):
# desde el Mac ese nombre no resuelve, solo a través del bastión.
kind get kubeconfig --internal --name "$KIND_NAME" > "$OUT/kind-internal.kubeconfig"

for _ in $(seq 1 50); do
  ssh -F "$OUT/ssh_config" -o BatchMode=yes oculus-it-bastion true 2> /dev/null && break
  sleep 0.2
done
echo "Bastión $NAME listo en 127.0.0.1:$PORT. Ficheros: $OUT"
echo "  OCULUS_IT_SSH_DIR=$OUT cargo test -p oculus-core --test kind tunel -- --ignored --nocapture"
