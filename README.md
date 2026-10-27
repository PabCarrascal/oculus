# Oculus

IDE de escritorio para Kubernetes: Tauri 2 + Rust (kube-rs) + React. El diseño completo está en [docs/](docs/README.md).

**Estado:** etapa 1 (clústeres). Lee tus kubeconfig (`~/.kube/config`, `KUBECONFIG` y ficheros añadidos a mano), conecta a varios clústeres a la vez con exec plugins (aws, kubelogin…), muestra el estado y el discovery de cada uno y guarda preferencias por clúster (alias, color, solo lectura, protegido).

Etapa 2 (explorador): tablas en vivo de cualquier tipo de recurso, CRDs incluidos, con filtro (`label:app=web`), orden, selección, columnas configurables y filtro de namespaces. Todavía no hay vista de detalle ni acciones.

## Requisitos

- Rust: la versión se fija en `rust-toolchain.toml` y rustup la instala sola.
- pnpm 12. Node no hace falta instalarlo: `devEngines` en `package.json` hace que pnpm descargue Node 24 solo para este proyecto.
- macOS: Xcode Command Line Tools. Linux: `libwebkit2gtk-4.1-dev libayatana-appindicator3-dev librsvg2-dev libxdo-dev`. Windows: WebView2 (viene con Windows 11).

## Comandos

```bash
pnpm install
```

```bash
pnpm dev
```

| Comando | Qué hace |
|---|---|
| `pnpm dev` | App en modo desarrollo con recarga en caliente |
| `pnpm build` | Instaladores de release en `target/release/bundle/` |
| `pnpm test` | Tests del frontend (Vitest) |
| `cargo test --workspace` | Tests de Rust |
| `pnpm lint` | Biome + comprobación de tipos |
| `pnpm gen:bindings` | Regenera `packages/bindings` a partir de los tipos de Rust |

Los logs del backend se filtran con `OCULUS_LOG` (por ejemplo `OCULUS_LOG=debug pnpm dev`).

`pnpm --filter @oculus/desktop dev:web` abre la UI en un navegador con un backend simulado (`src/dev/browserMock.ts`), sin Tauri. Solo existe en desarrollo.

### Tests contra un clúster real

```bash
tests/fixtures/kind/setup.sh /tmp/oculus-it.kubeconfig
```

```bash
OCULUS_IT_KUBECONFIG=/tmp/oculus-it.kubeconfig cargo test -p oculus-core --test kind -- --ignored --nocapture
```

`setup.sh` crea un clúster kind con 100 CRDs y escribe su kubeconfig en un fichero aparte: nunca toca `~/.kube/config` ni `KUBECONFIG`.

El túnel SSH se prueba además con un bastión en la red Docker de kind:

```bash
tests/fixtures/ssh/setup.sh /tmp/oculus-it-ssh
```

```bash
OCULUS_IT_KUBECONFIG=/tmp/oculus-it.kubeconfig OCULUS_IT_SSH_DIR=/tmp/oculus-it-ssh cargo test -p oculus-core --test kind -- --ignored --nocapture
```

La clave, el `known_hosts` y el `ssh_config` de la prueba se generan en ese directorio y se pasan con `ssh -F`: no se lee ni se toca `~/.ssh` ni el agente.

### Escala (10.000 pods con KWOK)

```bash
tests/fixtures/kwok/setup.sh /tmp/oculus-perf.kubeconfig 10000
```

```bash
OCULUS_SCALE_KUBECONFIG=/tmp/oculus-perf.kubeconfig cargo test -p oculus-core --release --test scale -- --ignored --nocapture
```

Para la UI, `pnpm --filter @oculus/desktop preview:perf` sirve la build de producción con el backend simulado en `http://localhost:1421`; añade `?rows=10000&churn=500` a la URL del explorador.

### Arranque

```bash
python3 tools/measure-startup.py
```

Mide el arranque de la build de release desde fuera, incluida la carga del proceso, y la medida interna de la app.

### Dependencias npm y el lockfile

`pnpm-lock.yaml` no lleva URLs de ningún registro. Si tu pnpm usa un mirror, añade dependencias así para que no se cuelen sus URLs:

```bash
pnpm add <paquete> --config.registry=https://registry.npmjs.org/
```

## Estructura

```
apps/desktop/          app Tauri: src-tauri/ (Rust, capa fina) y src/ (React)
crates/oculus-protocol tipos compartidos con la UI
crates/oculus-core     núcleo sin dependencia de Tauri: ajustes, kubeconfig, sesiones, discovery
packages/bindings      TS generado por tauri-specta + fachada con nombres limpios
packages/ui            design tokens y componentes sobre Radix
packages/i18n          catálogos es/en
```

Los crates `oculus-watch`, `oculus-ops` y `oculus-pty` de [docs/02-estructura.md](docs/02-estructura.md) se crean en la etapa que los necesita, no antes.
