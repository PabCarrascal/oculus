#!/usr/bin/env bash
# Crea un clúster kind desechable con 100 CRDs para los tests de integración.
#
#   tests/fixtures/kind/setup.sh [kubeconfig-de-salida]
#
# El kubeconfig se escribe en un fichero aparte: nunca toca ~/.kube/config ni KUBECONFIG.
set -euo pipefail

NAME="${OCULUS_KIND_NAME:-oculus-it}"
OUT="${1:-${TMPDIR:-/tmp}/oculus-it.kubeconfig}"
CRDS="${OCULUS_KIND_CRDS:-100}"

if ! kind get clusters 2>/dev/null | grep -qx "$NAME"; then
  kind create cluster --name "$NAME" --kubeconfig "$OUT" --wait 120s
else
  kind get kubeconfig --name "$NAME" > "$OUT"
fi

for i in $(seq 1 "$CRDS"); do
  cat <<EOF
---
apiVersion: apiextensions.k8s.io/v1
kind: CustomResourceDefinition
metadata:
  name: widgets${i}.g${i}.oculus.test
spec:
  group: g${i}.oculus.test
  scope: Namespaced
  names: { plural: widgets${i}, singular: widget${i}, kind: Widget${i} }
  versions:
  - name: v1
    served: true
    storage: true
    schema:
      openAPIV3Schema: { type: object, x-kubernetes-preserve-unknown-fields: true }
EOF
done | kubectl --kubeconfig "$OUT" apply -f - > /dev/null

kubectl --kubeconfig "$OUT" wait --for condition=established --timeout=120s crd -l '!nonexistent' > /dev/null
echo "Clúster $NAME listo con $CRDS CRDs. Kubeconfig: $OUT"
echo "  OCULUS_IT_KUBECONFIG=$OUT cargo test -p oculus-core --test kind -- --ignored --nocapture"
