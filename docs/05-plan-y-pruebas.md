# 05 · Plan por etapas y estrategia de pruebas

Cada etapa termina con un **hito verificable**: se demuestra en una demo y se mide en CI. Las cifras son objetivos iniciales; se ajustan cuando haya datos reales en la etapa 0.

**Máquina de referencia** para las medidas: portátil de gama media (4 núcleos, 16 GB, SSD), con un clúster KWOK de 10.000 pods y 500 nodos.

## Etapa 0 · Cimientos ✅

- Monorepo con Cargo y pnpm, `tauri-specta` generando bindings y la comprobación de bindings en CI.
- CI con lint y tests en los tres SO; build firmado en macOS y Windows (aunque al principio sea con certificados de prueba).
- Design system base: tokens, tema claro/oscuro, Radix y tipografía. i18n es/en.
- Ventana vacía con navegación, ajustes persistidos y la paleta de comandos vacía.

**Hito:** la app se instala y arranca en los tres SO. Arranque en frío (hasta la primera pintura con contenido) **< 1 s** con la app vacía. Esto deja ~1 s de margen para el objetivo de 2 s.

## Etapa 1 · Clústeres ✅

- Fuentes de kubeconfig (por defecto, `KUBECONFIG` y manuales), vigiladas con `notify`.
- Conexión bajo demanda, estado por clúster, discovery y preferencias (alias, color, solo lectura, protegido).
- Resolución del `PATH` de la shell de login al arrancar y rutas extra configurables.

**Hito:** conecta a kind (certificados), a EKS (exec plugin `aws`), a GKE (`gke-gcloud-auth-plugin`) y a AKS (`kubelogin`). Un clúster inalcanzable da error claro en **< 10 s** sin bloquear a los demás. Discovery en **< 1,5 s** en un clúster con 100 CRDs.

**Resultado (2026-10-26, kind v1.37 con 100 CRDs, Mac arm64):** discovery agregado en 4 ms (171 tipos), conexión completa en 12 ms, servidor inalcanzable con `Unreachable` en 5,0 s. Pendiente: probar EKS, GKE y AKS de verdad. Los exec plugins solo se han probado con binarios simulados en los tests.

## Etapa 2 · Explorador (solo lectura) ✅

- WatchManager: reflectors, recuento de referencias, linger, proyección a filas, lotes de deltas y watchers por namespace cuando listar todo da 403.
- Columnas de los tipos nativos y de los CRDs (`additionalPrinterColumns`).
- Tabla virtualizada con filtros, búsqueda, ordenación, selección múltiple y columnas configurables.

**Hito (con KWOK, 10.000 pods):**
- Primer snapshot pintado en **< 1,5 s** desde que se abre la vista.
- Scroll a **60 fps** (ningún frame > 16 ms en el perfil de Chrome DevTools o Safari Web Inspector).
- 500 cambios/s durante 60 s sin que la UI se bloquee (ninguna tarea > 50 ms).
- Memoria del proceso Rust **< 150 MB** por clúster con esa vista abierta. Webview **< 250 MB**.
- Cerrar la vista libera el watcher en 30 s ± 1 s (se comprueba con métricas internas).

**Resultado (2026-10-26, Mac arm64):**

| Medida | Objetivo | Resultado |
|---|---|---|
| Snapshot de 10.000 pods (KWOK, API server real) | < 1,5 s | 435 ms |
| Memoria del backend con esa vista | < 150 MB | +70 MB de RSS (incluye la copia de filas del propio test) |
| 500 cambios/s durante 60 s | sin bloqueos | 30.001 parches, 751 lotes de 80 ms, 0 filas desfasadas, RSS estable |
| Scroll con 10.000 filas y 500 cambios/s (UI, build de producción en Chrome) | 60 fps | 120 fps, ningún frame > 9 ms, 0 tareas largas en 30 s, heap 23–44 MB |
| Linger al cerrar la vista | 30 s ± 1 | test unitario con tiempo simulado |

Pendiente: la memoria del webview real (WKWebView) y los fps dentro de la app, porque la UI se midió en Chrome con el backend simulado. En modo desarrollo de React los fps bajan visiblemente (p95 de 25 ms), así que las medidas de rendimiento deben hacerse siempre con la build de producción (`preview:perf`).

## Etapa 3 · Detalle y YAML ✅

- Panel de detalle con resumen por tipo, eventos y relacionados.
- Editor Monaco con carga diferida, esquemas de `/openapi/v3`, dry-run, apply, diff y aviso de conflicto si el objeto cambió.
- Secrets: valores ocultos y `secret_reveal` explícito.

**Hito:** editar y aplicar un Deployment; un error de validación marca la línea correcta; un conflicto (409) ofrece ver diferencias. La primera apertura del editor tarda **< 800 ms** y no afecta al arranque (medido).

**Resultado (2026-10-27, contra kind):** editar y aplicar un Deployment funciona, también en pleno escalado (con PUT daba conflicto; ver 03). Un campo desconocido (`spec.replicass`) y un valor inválido (`spec.replicas: -1`) vuelven con su ruta exacta y se marcan en su línea. Un cambio ajeno en el mismo campo da conflicto con diferencias y "sobrescribir". Primera apertura del editor: 323 ms en la build de producción (Monaco 0.52 y monaco-yaml, unos 3,5 MB en bloques aparte que no se descargan hasta abrir la pestaña). Arranque: ~400 ms. Mientras se escribe, el esquema real de `apps/v1` marca tipos incorrectos en su línea, también a varios `$ref` de profundidad (`replicas: tres`, `containerPort: "80"`). En la app real hubo que ajustar la CSP de Tauri (estilos y workers) y bajar a Monaco 0.52, porque el validador no arrancaba (ver D6 en 01). Con eso, la app real carga el esquema y el log queda sin errores de workers ni de esquema.

## Añadido · Túnel SSH por clúster ✅

Antes de la etapa 4, para que exec y port-forward nazcan ya probados a través del túnel. Diseño en D13 (01) y en 03.

- Preferencia `sshTunnel.host` en el diálogo del clúster, con «Probar» (`ssh -G`, sin conectar) y aviso si el kubeconfig ya trae `proxy-url`.
- Túnel `ssh -D` compartido por host, con paso «tunnel» en la conexión, reconexión en el mismo puerto e indicador en la barra lateral y en la de estado.
- Feature `kube/socks5`. Pruebas unitarias de la validación del host, la línea de órdenes y la clasificación de la salida de error de `ssh`.

**Hito (contra kind):** un contenedor `sshd` (imagen propia sobre Alpine, sin imágenes de terceros) en la red Docker `kind`, con clave y `known_hosts` generados para la prueba y pasados con `-F` (nunca el `~/.ssh` del usuario). El kubeconfig de `kind get kubeconfig --internal` apunta a `https://oculus-it-control-plane:6443`, un nombre que el Mac no puede resolver; el certificado de kind lo incluye. Conectar por el túnel, abrir el detalle y aplicar un cambio funcionan. Al matar el `ssh`, la sesión pasa a «reconnecting» y vuelve sola **sin crear otro `Client`**, con los watchers al día. Un host sin la clave da `Unauthorized` con su pista en **< 15 s**.

**Resultado (2026-10-27, kind + bastión Alpine):**
- Sin túnel, el nombre interno del API server no resuelve, como se esperaba.
- Un usuario sin clave da `Unauthorized` con la pista de `ssh-add` en ~300 ms.
- Por el túnel, la conexión completa tarda 177 ms. El detalle y un dry-run funcionan.
- Al matar `ssh`, la sesión pasa a «reconnecting» y vuelve en 1,1 s sin reconectar el clúster: el mismo `Client`, sin estados `connecting`. El detalle y los namespaces responden después.
- Al desconectar, `ssh` desaparece en ~100 ms.

En la app real (build de producción, con el agente y el `~/.ssh` del usuario), el túnel se abre en 114 ms y el clúster conecta por él, sin errores en el log. Esta prueba destapó que `ssh` no entiende `usuario@host:puerto`; ahora se le pasa como `ssh://` (ver 03).

Pendiente:
- en la app real, la reconexión al matar `ssh` y un bastión de verdad del usuario;
- un bastión con `ProxyJump`;
- Windows (OpenSSH de serie y huérfanos).

## Etapa 4 · Operaciones

- Logs (multicontenedor y por workload, seguir, buscar, nivel, descarga) y exec con xterm.js.
- Port-forward a pod y a service, con reconexión y lista global.
- Escalar, reiniciar rollout y borrar con confirmación (con nombre escrito si el clúster está protegido).
- Filtrado por RBAC de todas las acciones.

**Hito:** con un usuario de prueba de solo lectura, ninguna acción de escritura está habilitada y ninguna llamada devuelve un 403 inesperado. Un log de 100.000 líneas permite buscar en **< 200 ms**. Un port-forward sobrevive a la muerte del pod detrás de un Service.

## Etapa 5 · Terminal, atajos y pulido

- Terminal local (D8), dock con pestañas y atajos para todo, documentados en la propia paleta.
- Auditoría de accesibilidad (teclado, lectores de pantalla, contraste AA, _reducir movimiento_).
- Catálogos i18n completos.

**Hito:** en la terminal, `kubectl config current-context` devuelve el contexto de la pestaña y no aparece **ningún** fichero nuevo con credenciales en disco (comprobado por un test). Todas las acciones del MVP se pueden hacer sin ratón.

## Etapa 6 · Release 1.0

- Auto-actualización firmada (`tauri-plugin-updater`), notarización en macOS y firma en Windows.
- Paquetes: `.dmg`, `.msi`/`.exe`, `.AppImage`, `.deb` y `.rpm`.
- Informes de errores opcionales (desactivados por defecto) con redacción de datos sensibles.

**Hito:** instalación limpia y actualización desde la versión anterior en los tres SO, y prueba de humo E2E verde en cada release.

Después de la 1.0, la **fase 2 del producto**, en este orden orientativo: métricas → Helm → topología → extensiones → asistente IA.

---

## Estrategia de pruebas

| Nivel                   | Qué cubre                                                                                                          | Herramientas                                                                            | Dónde se ejecuta                                 |
| ----------------------- | ------------------------------------------------------------------------------------------------------------------ | --------------------------------------------------------------------------------------- | ------------------------------------------------ |
| **Unitarias Rust**      | Proyección a filas, fusión de kubeconfig, derivación de estado, caché RBAC, lotes de deltas, redacción de secretos | `cargo test` + `Client` de kube-rs construido sobre un servicio simulado (`tower-test`) | Cada PR, 3 SO                                    |
| **Unitarias UI**        | Stores de suscripción (snapshot/delta/relist), componentes y formateo                                              | Vitest + Testing Library, IPC simulado con `@tauri-apps/api/mocks`                      | Cada PR                                          |
| **Integración**         | Watch real, 410 forzado, RBAC con usuarios limitados, exec, port-forward, apply y dry-run                          | kind/k3d efímeros en CI, con manifiestos de `tests/fixtures`                            | Cada PR (Linux)                                  |
| **Escala**              | Objetivos de memoria, latencia y throughput de la etapa 2                                                          | **KWOK**: miles de nodos y pods simulados sin cargas reales                             | Nightly + antes de cada release                  |
| **E2E con la app real** | Flujos completos: conectar, explorar, editar, logs y terminal                                                      | WebdriverIO + `tauri-driver`                                                            | Linux y Windows en CI                            |
| **E2E en macOS**        | Mismos flujos, solo la UI                                                                                          | Playwright contra Vite con IPC simulado                                                 | CI (macOS) + prueba manual antes de cada release |
| **Contrato**            | Los bindings TS están sincronizados con Rust                                                                       | `pnpm gen:bindings && git diff --exit-code`                                             | Cada PR                                          |

**Notas:**
- `tauri-driver` **no funciona en macOS**, porque no existe un driver WebDriver para WKWebView. Por eso macOS tiene su propia vía con Playwright y una lista de comprobación manual corta antes de cada release.
- Los tests de integración usan una **matriz de versiones** de Kubernetes: las tres últimas minor, más la más antigua que se quiera soportar.
- Los **cortes de red** se provocan con un proxy de fallos (Toxiproxy) entre la app y el API server.
- El **410 Gone** no lo puede generar un proxy: lo devuelve el servidor cuando el `resourceVersion` ya está compactado. Se prueba con el `Client` simulado en los tests unitarios y, en integración, reanudando un watch desde un `resourceVersion` antiguo después de forzar la compactación de etcd.
- Hay un test que **falla si se escribe en disco** cualquier valor de token, certificado o Secret: revisa los logs y el directorio de datos de la app tras una sesión de prueba.
