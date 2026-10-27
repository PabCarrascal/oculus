# Oculus · Diseño

Oculus es una aplicación de escritorio (macOS, Windows, Linux) para gestionar clústeres de Kubernetes con interfaz gráfica, inspirada en Lens/OpenLens pero más ligera: **Tauri 2 + Rust (kube-rs)** en el backend y **React + TypeScript** en la UI.

Este directorio contiene el diseño. La implementación va por la **etapa 0** (cimientos); el estado y los comandos de desarrollo están en el [README del repositorio](../README.md).

| Documento                                   | Contenido                                                                                                     |
| ------------------------------------------- | ------------------------------------------------------------------------------------------------------------- |
| [01 · Arquitectura](01-arquitectura.md)     | Componentes, flujo de datos, modelo de estado y decisiones técnicas (D1–D13) con las alternativas descartadas |
| [02 · Estructura](02-estructura.md)         | Monorepo, crates, paquetes y convenciones                                                                     |
| [03 · Contrato IPC](03-contrato-ipc.md)     | Comandos, channels, eventos y tipos entre Rust y la UI                                                        |
| [04 · Wireframes](04-wireframes.md)         | Selector de clústeres, lista, detalle, YAML, logs, terminal y paleta de comandos                              |
| [05 · Plan y pruebas](05-plan-y-pruebas.md) | Etapas 0–6 con hitos medibles y estrategia de pruebas                                                         |
| [06 · Riesgos](06-riesgos.md)               | Doce riesgos principales y cómo mitigarlos                                                                    |

## Decisiones que conviene revisar antes de aprobar

1. **Tauri 2 frente a Electron** (D1). Más ligero, pero exige probar en tres webviews distintos.
2. **Logs en una lista virtualizada en vez de xterm.js** (D7). Es una desviación del planteamiento inicial.
3. **La terminal no copia credenciales** (D8): usa un kubeconfig mínimo que solo fija el contexto, en lugar del proxy local de Lens.
4. **El nombre "Oculus"**: revisar posibles conflictos de marca antes de publicar (riesgo 11).
