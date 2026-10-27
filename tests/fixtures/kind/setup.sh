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

# Datos variados para probar el explorador a mano: estados distintos, un Secret
# y un CRD con columnas propias (additionalPrinterColumns).
if [ "${OCULUS_KIND_SAMPLES:-1}" = "1" ]; then
  K="kubectl --kubeconfig $OUT"
  $K create namespace demo --dry-run=client -o yaml | $K apply -f - > /dev/null
  $K -n demo create deployment web --image=registry.k8s.io/pause:3.10 --replicas=3 --dry-run=client -o yaml | $K apply -f - > /dev/null
  $K -n demo create deployment crash --image=busybox:1.37 --dry-run=client -o yaml -- sh -c 'echo arrancando; exit 1' | $K apply -f - > /dev/null
  $K -n demo create secret generic credenciales --from-literal=token=no-es-real --dry-run=client -o yaml | $K apply -f - > /dev/null
  $K apply -f - > /dev/null <<'YAML'
apiVersion: apiextensions.k8s.io/v1
kind: CustomResourceDefinition
metadata: { name: gadgets.demo.oculus.test }
spec:
  group: demo.oculus.test
  scope: Namespaced
  names: { plural: gadgets, singular: gadget, kind: Gadget }
  versions:
  - name: v1
    served: true
    storage: true
    schema:
      openAPIV3Schema: { type: object, x-kubernetes-preserve-unknown-fields: true }
    additionalPrinterColumns:
    - { name: Color, type: string, jsonPath: .spec.color }
    - { name: Tamaño, type: integer, jsonPath: .spec.size }
    - { name: Listo, type: string, jsonPath: '.status.conditions[?(@.type=="Ready")].status' }
    - { name: Age, type: date, jsonPath: .metadata.creationTimestamp }
YAML
  $K wait --for condition=established --timeout=60s crd/gadgets.demo.oculus.test > /dev/null
  for g in "rojo 3 True" "verde 7 False" "azul 12 True"; do
    set -- $g
    $K -n demo apply -f - > /dev/null <<YAML
apiVersion: demo.oculus.test/v1
kind: Gadget
metadata: { name: gadget-$1 }
spec: { color: $1, size: $2 }
status: { conditions: [{ type: Ready, status: "$3" }] }
YAML
  done
  echo "Datos de ejemplo en el namespace demo (Deployments web y crash, Secret, Gadgets)."
fi
