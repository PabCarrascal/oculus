#!/usr/bin/env bash
# Clúster KWOK con miles de pods simulados para las medidas de escala de la etapa 2.
#
#   tests/fixtures/kwok/setup.sh [kubeconfig-de-salida] [pods]
#
# KWOK simula nodos y pods sin contenedores reales: 10.000 pods caben en un portátil.
# El kubeconfig va a un fichero aparte: nunca toca ~/.kube/config ni KUBECONFIG.
set -euo pipefail

NAME="${OCULUS_KWOK_NAME:-oculus-perf}"
OUT="${1:-${TMPDIR:-/tmp}/oculus-perf.kubeconfig}"
PODS="${2:-10000}"
NODES=100
NAMESPACES=20
K="kubectl --kubeconfig $OUT"

if ! kwokctl get clusters 2>/dev/null | grep -qx "$NAME"; then
  kwokctl create cluster --name "$NAME" --runtime docker --kubeconfig "$OUT" --disable-qps-limits
else
  kwokctl get kubeconfig --name "$NAME" > "$OUT"
fi

for n in $(seq 0 $((NODES - 1))); do
  cat <<YAML
---
apiVersion: v1
kind: Node
metadata:
  name: kwok-node-$n
  annotations: { kwok.x-k8s.io/node: fake }
  labels: { type: kwok, kubernetes.io/hostname: kwok-node-$n }
spec:
  taints: [{ key: kwok.x-k8s.io/node, value: fake, effect: NoSchedule }]
YAML
done | $K apply -f - > /dev/null

for ns in $(seq 0 $((NAMESPACES - 1))); do echo "apiVersion: v1
kind: Namespace
metadata: { name: team-$ns }
---"; done | $K apply -f - > /dev/null

# Pods asignados directamente a nodos falsos (sin pasar por el scheduler), en lotes.
BATCH=1000
for start in $(seq 0 $BATCH $((PODS - 1))); do
  end=$((start + BATCH - 1)); [ $end -ge $PODS ] && end=$((PODS - 1))
  for i in $(seq $start $end); do
    cat <<YAML
---
apiVersion: v1
kind: Pod
metadata:
  name: app-$i
  namespace: team-$((i % NAMESPACES))
  labels: { app: app-$((i % 97)), tier: $([ $((i % 2)) -eq 0 ] && echo web || echo worker) }
spec:
  nodeName: kwok-node-$((i % NODES))
  tolerations: [{ key: kwok.x-k8s.io/node, operator: Exists, effect: NoSchedule }]
  containers: [{ name: app, image: fake }]
YAML
  done | $K create -f - > /dev/null 2>&1 || true
done

echo "Pods creados: $($K get pods -A --no-headers 2>/dev/null | wc -l | tr -d ' ')"
echo "  OCULUS_SCALE_KUBECONFIG=$OUT cargo test -p oculus-core --release --test scale -- --ignored --nocapture"
