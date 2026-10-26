# 04 · Wireframes

Distribución general: **barra de clústeres** a la izquierda (siempre visible), **navegación de recursos**, **área principal** con pestañas, **panel de detalle** deslizante a la derecha y **dock inferior** para logs, exec y terminal. Todo se puede hacer con teclado; `⌘K` / `Ctrl+K` abre la paleta de comandos desde cualquier sitio.

## 1. Inicio y selector de clústeres

Es la pantalla al abrir la app sin ningún clúster conectado. También se llega desde el icono 🏠 de la barra izquierda.

```
┌────┬──────────────────────────────────────────────────────────────────────────────────┐
│ 🏠 │  Clústeres                                     [ Buscar contexto…    ]  [+ Añadir] │
│────│                                                                                    │
│ ●P │  ~/.kube/config                                                                    │
│ ●S │  ┌──────────────────────────────────────────────────────────────────────────────┐  │
│ ○D │  │ ● prod-eu        eks · https://A1B2.eks.amazonaws.com   v1.31  🔒 protegido  │  │
│    │  │ ● staging        gke · https://34.90.1.2                v1.32                │  │
│    │  │ ○ docker-desktop local · https://127.0.0.1:6443          —     [Conectar]    │  │
│    │  │ ⚠ old-cluster    Unreachable: timeout tras 10 s          [Reintentar] [···]  │  │
│    │  └──────────────────────────────────────────────────────────────────────────────┘  │
│    │                                                                                    │
│    │  ~/work/kubeconfigs/clientes.yaml                                    [Quitar]      │
│    │  ┌──────────────────────────────────────────────────────────────────────────────┐  │
│    │  │ ○ cliente-a      https://k8s.cliente-a.com              —     [Conectar]    │  │
│    │  └──────────────────────────────────────────────────────────────────────────────┘  │
│ ⚙  │                                                                                    │
└────┴──────────────────────────────────────────────────────────────────────────────────┘
 ● conectado   ○ desconectado   ⚠ error   Barra izquierda: clústeres anclados (iniciales y color)
```

- `[···]` abre: alias, color, solo lectura, protegido y namespace por defecto.
- Un error muestra la pista concreta (por ejemplo, _"No se encontró `gke-gcloud-auth-plugin` en el PATH → Ajustes › Rutas"_).

## 2. Lista de recursos

```
┌────┬──────────────────┬────────────────────────────────────────────────────────────────┐
│ 🏠 │ prod-eu      ▾   │ Pods ×  Deployments ×  +                                        │
│────│                  │────────────────────────────────────────────────────────────────│
│ ●P │ ▸ Resumen        │ Namespaces: [ default, payments  ▾ ]  [ Filtrar… /         ]  ⚙  │
│ ●S │ ▾ Workloads      │                                                     1.284 pods │
│    │   Pods      1284 │ ☐ Nombre ▲               Namespace  Estado      Reinic.  Edad   │
│    │   Deployments 42 │ ─────────────────────────────────────────────────────────────── │
│    │   StatefulSets 6 │ ☐ api-7d9f8-x2kq         payments   ● Running        0   3d     │
│    │   DaemonSets   4 │ ☐ api-7d9f8-zz81         payments   ● Running        2   3d     │
│    │   Jobs        12 │ ☑ worker-5c4b-a1         payments   ▲ CrashLoop     17   2h     │
│    │   CronJobs     3 │ ☑ worker-5c4b-b7         payments   ▲ CrashLoop     15   2h     │
│    │ ▸ Config         │ ☐ web-6f7a-q9            default    ◌ Pending        0   12s    │
│    │ ▸ Red            │ ☐ …                                                             │
│    │ ▸ Almacenamiento │                                                                 │
│    │ ▸ Control acceso │ ┌ 2 seleccionados ─ [Logs] [Borrar…] ───────────────────────┐   │
│    │ ▸ Clúster        │ └───────────────────────────────────────────────────────────┘   │
│    │ ▸ CRDs (27)      │                                                                 │
├────┴──────────────────┴────────────────────────────────────────────────────────────────┤
│ ● prod-eu · v1.31 · watch: listo · 3 port-forwards                    ⌘K  Paleta        │
└──────────────────────────────────────────────────────────────────────────────────────────┘
```

- Las filas cambian en vivo; la que cambia parpadea brevemente (se desactiva con _reducir movimiento_).
- `⚙` elige qué columnas se ven. `/` enfoca el filtro, que admite texto libre y `label:app=api`.
- Si el watch está re-listando, la barra inferior lo dice y la tabla sigue mostrando los últimos datos.
- Las acciones sin permiso RBAC aparecen deshabilitadas, con un tooltip que explica el motivo.

## 3. Detalle de un recurso

Se abre a la derecha sin perder la tabla. Con `⇧Enter` se abre en pestaña completa.

```
┌──────────────────────────────────────────────┬─────────────────────────────────────────┐
│ (tabla de pods, atenuada)                    │ Pod  worker-5c4b-a1               [✕]   │
│                                              │ payments · ▲ CrashLoopBackOff           │
│                                              │ [Logs] [Shell] [Port-forward] [Editar]  │
│                                              │ [Borrar…]                               │
│                                              │─────────────────────────────────────────│
│                                              │ Resumen │ YAML │ Eventos (5) │ Relac.   │
│                                              │─────────────────────────────────────────│
│                                              │ Nodo         ip-10-0-3-12               │
│                                              │ IP           10.0.3.88                  │
│                                              │ QoS          Burstable                  │
│                                              │ Controlado   ReplicaSet worker-5c4b ↗   │
│                                              │ Labels       app=worker  tier=bg        │
│                                              │                                         │
│                                              │ Contenedores                            │
│                                              │ ▲ worker  img: reg/worker:1.8.2         │
│                                              │   Último fin: Error (exit 1) hace 40 s  │
│                                              │   CPU 100m/500m  Mem 128Mi/256Mi        │
│                                              │                                         │
│                                              │ Eventos recientes                       │
│                                              │ ⚠ BackOff  Back-off restarting… ×17     │
└──────────────────────────────────────────────┴─────────────────────────────────────────┘
```

**Pestaña YAML (modo edición):**

```
│ YAML                                   [Validar (dry-run)]  [Descartar]  [Aplicar ⌘S]  │
│─────────────────────────────────────────────────────────────────────────────────────────│
│  12 │   replicas: 3                                                                     │
│  13 │   template:                                                                       │
│  14 │     spec:                                                                         │
│  15 │       containers:                                                                 │
│  16 │       - name: worker                                                              │
│  17 │         imagePullPolicy: Sometimes   ⚠ Debe ser Always, IfNotPresent o Never      │
│─────────────────────────────────────────────────────────────────────────────────────────│
│ ⚠ El recurso cambió en el clúster mientras editabas.   [Ver diferencias]  [Recargar]     │
```

## 4. Logs (dock inferior)

```
├─────────────────────────────────────────────────────────────────────────────────────────┤
│ Logs: deploy/worker ×  │  Shell: api-7d9f8-x2kq ×  │  Terminal: prod-eu ×  │  +    ▁ ▢ ✕  │
│─────────────────────────────────────────────────────────────────────────────────────────│
│ Pods: [todos (3) ▾]  Cont.: [worker ▾]  [Buscar…        ] ☑ Seguir ☐ Hora ☐ Anterior    │
│ Nivel: [● error ● warn ○ info]                       ⤓ Descargar   ⏸ Pausar   🗑 Limpiar │
│─────────────────────────────────────────────────────────────────────────────────────────│
│ a1 │ 12:01:03 INFO  worker started, queue=payments                                     │
│ b7 │ 12:01:03 INFO  worker started, queue=payments                                     │
│ a1 │ 12:01:04 ERROR cannot connect to redis:6379: connection refused                   │
│ a1 │ 12:01:04 ERROR fatal: exiting                                                     │
│ ── a1 reiniciado (reconectando…) ──────────────────────────────────────────────────────│
│ a1 │ 12:01:44 INFO  worker started, queue=payments                                     │
│                                                              48.211 líneas · ↓ al final │
```

- Cada pod lleva un color en la columna de la izquierda. Si se desactiva _Seguir_, se puede navegar por el historial sin que la vista salte.

## 5. Terminal integrada / exec

```
├─────────────────────────────────────────────────────────────────────────────────────────┤
│ Logs: deploy/worker ×  │  Shell: api-7d9f8-x2kq ×  │  Terminal: prod-eu ×  │  +    ▁ ▢ ✕  │
│─────────────────────────────────────────────────────────────────────────────────────────│
│ ⎈ prod-eu · ns: payments                                                    zsh  ▾      │
│                                                                                         │
│ ~/work $ kubectl get deploy                                                             │
│ NAME     READY   UP-TO-DATE   AVAILABLE   AGE                                           │
│ api      2/2     2            2           3d                                            │
│ worker   1/3     3            1           3d                                            │
│ ~/work $ █                                                                              │
```

- `Terminal:` es una shell local con `kubectl` ya apuntando al contexto (D8). `Shell:` es un exec dentro de un contenedor.
- En un clúster **protegido**, la cabecera de la terminal se pinta del color del clúster para que no se confunda con otro.

## Paleta de comandos (`⌘K`)

```
            ┌──────────────────────────────────────────────────────┐
            │ › worker                                             │
            │──────────────────────────────────────────────────────│
            │ ▸ Pod  worker-5c4b-a1          prod-eu / payments    │
            │ ▸ Deployment  worker           prod-eu / payments    │
            │ ⚡ Logs de deploy/worker                              │
            │ ⚡ Reiniciar rollout de deploy/worker                 │
            │ ⎈ Cambiar a clúster "staging"                         │
            └──────────────────────────────────────────────────────┘
```

La paleta busca recursos (de todos los watchers activos), acciones sobre ellos, clústeres y ajustes. Cada acción muestra su atajo si lo tiene.
