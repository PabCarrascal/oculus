# 05 · Plan por etapas y estrategia de pruebas

Cada etapa termina con un **hito verificable**: se demuestra en una demo y se mide en CI. Las cifras son objetivos iniciales; se ajustan cuando haya datos reales en la etapa 0.

**Máquina de referencia** para las medidas: portátil de gama media (4 núcleos, 16 GB, SSD), con un clúster KWOK de 10.000 pods y 500 nodos.

## Etapa 0 · Cimientos

- Monorepo con Cargo y pnpm, `tauri-specta` generando bindings y la comprobación de bindings en CI.
- CI con lint y tests en los tres SO; build firmado en macOS y Windows (aunque al principio sea con certificados de prueba).
- Design system base: tokens, tema claro/oscuro, Radix y tipografía. i18n es/en.
- Ventana vacía con navegación, ajustes persistidos y la paleta de comandos vacía.

**Hito:** la app se instala y arranca en los tres SO. Arranque en frío (hasta la primera pintura con contenido) **< 1 s** con la app vacía. Esto deja ~1 s de margen para el objetivo de 2 s.

## Etapa 1 · Clústeres

- Fuentes de kubeconfig (por defecto, `KUBECONFIG` y manuales), vigiladas con `notify`.
- Conexión bajo demanda, estado por clúster, discovery y preferencias (alias, color, solo lectura, protegido).
- Resolución del `PATH` de la shell de login al arrancar y rutas extra configurables.

**Hito:** conecta a kind (certificados), a EKS (exec plugin `aws`), a GKE (`gke-gcloud-auth-plugin`) y a AKS (`kubelogin`). Un clúster inalcanzable da error claro en **< 10 s** sin bloquear a los demás. Discovery en **< 1,5 s** en un clúster con 100 CRDs.

## Etapa 2 · Explorador (solo lectura)

- WatchManager: reflectors, recuento de referencias, linger, proyección a filas, lotes de deltas y watchers por namespace cuando listar todo da 403.
- Columnas de los tipos nativos y de los CRDs (`additionalPrinterColumns`).
- Tabla virtualizada con filtros, búsqueda, ordenación, selección múltiple y columnas configurables.

**Hito (con KWOK, 10.000 pods):**
- Primer snapshot pintado en **< 1,5 s** desde que se abre la vista.
- Scroll a **60 fps** (ningún frame > 16 ms en el perfil de Chrome DevTools o Safari Web Inspector).
- 500 cambios/s durante 60 s sin que la UI se bloquee (ninguna tarea > 50 ms).
- Memoria del proceso Rust **< 150 MB** por clúster con esa vista abierta. Webview **< 250 MB**.
- Cerrar la vista libera el watcher en 30 s ± 1 s (se comprueba con métricas internas).

## Etapa 3 · Detalle y YAML

- Panel de detalle con resumen por tipo, eventos y relacionados.
- Editor Monaco con carga diferida, esquemas de `/openapi/v3`, dry-run, apply, diff y aviso de conflicto si el objeto cambió.
- Secrets: valores ocultos y `secret_reveal` explícito.

**Hito:** editar y aplicar un Deployment; un error de validación marca la línea correcta; un conflicto (409) ofrece ver diferencias. La primera apertura del editor tarda **< 800 ms** y no afecta al arranque (medido).

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
