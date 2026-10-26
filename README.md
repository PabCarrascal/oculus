# Oculus

IDE de escritorio para Kubernetes: Tauri 2 + Rust (kube-rs) + React. El diseño completo está en [docs/](docs/README.md).

**Estado:** etapa 0 (cimientos). La app arranca con navegación, ajustes persistidos, tema claro/oscuro, i18n es/en y paleta de comandos. Todavía no se conecta a clústeres.

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

## Estructura

```
apps/desktop/          app Tauri: src-tauri/ (Rust, capa fina) y src/ (React)
crates/oculus-protocol tipos compartidos con la UI
crates/oculus-core     núcleo sin dependencia de Tauri (de momento: ajustes)
packages/bindings      TS generado por tauri-specta + fachada con nombres limpios
packages/ui            design tokens y componentes sobre Radix
packages/i18n          catálogos es/en
```

Los crates `oculus-watch`, `oculus-ops` y `oculus-pty` de [docs/02-estructura.md](docs/02-estructura.md) se crean en la etapa que los necesita, no antes.
