#!/usr/bin/env python3
"""Mide el arranque en frío de Oculus (hito de la etapa 0).

Lanza el binario varias veces y da dos medidas por ejecución:
  - externa: desde que este script lanza el proceso hasta que el backend
    registra la primera pintura. Incluye la carga del proceso antes de main().
  - interna: la que calcula la propia app desde main() (startup_report).

    python3 tools/measure-startup.py [ruta-al-binario] [--runs 5]

Por defecto usa el binario dentro del .app/bundle de `pnpm build`.
"""
from __future__ import annotations

import argparse
import os
import queue
import re
import subprocess
import threading
import sys
import time
from pathlib import Path

DEFAULTS = {
    "darwin": "target/release/bundle/macos/Oculus.app/Contents/MacOS/oculus-desktop",
    "linux": "target/release/oculus-desktop",
    "win32": "target/release/oculus-desktop.exe",
}


def run_once(binary: str, timeout: float) -> tuple[float, float | None]:
    env = dict(os.environ, OCULUS_LOG="info", NO_COLOR="1")
    start = time.monotonic()
    proc = subprocess.Popen([binary], stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True, env=env)
    internal = None
    lines: queue.Queue[str | None] = queue.Queue()

    def reader() -> None:
        for raw in proc.stdout:
            lines.put(raw)
        lines.put(None)

    # Un hilo lee la salida y aquí se espera con timeout: si la app nunca escribe
    # la línea (por ejemplo, con la ventana oculta el webview no pinta), no nos
    # quedamos colgados.
    threading.Thread(target=reader, daemon=True).start()
    try:
        while (left := timeout - (time.monotonic() - start)) > 0:
            try:
                line = lines.get(timeout=left)
            except queue.Empty:
                break
            if line is None or "panicked" in line:
                break
            if "primera pintura" in line:
                m = re.search(r'ms="?([0-9.]+)', re.sub(r"\x1b\[[0-9;]*m", "", line))
                internal = float(m.group(1)) if m else None
                break
        external = (time.monotonic() - start) * 1000
    finally:
        proc.terminate()
        proc.wait(timeout=10)
    return external, internal


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("binary", nargs="?", default=DEFAULTS.get(sys.platform))
    parser.add_argument("--runs", type=int, default=5)
    parser.add_argument("--timeout", type=float, default=20.0)
    args = parser.parse_args()
    if not args.binary or not Path(args.binary).exists():
        print(f"No existe el binario: {args.binary}. Ejecuta antes `pnpm build`.", file=sys.stderr)
        return 1

    results = []
    for i in range(args.runs):
        external, internal = run_once(args.binary, args.timeout)
        results.append(external)
        shown = f"{internal:.0f} ms" if internal is not None else "sin dato (¿ventana oculta?)"
        print(f"ejecución {i + 1}: externa {external:.0f} ms | interna {shown}")
    results.sort()
    print(f"externa: mejor {results[0]:.0f} ms, mediana {results[len(results) // 2]:.0f} ms, peor {results[-1]:.0f} ms")
    return 0


if __name__ == "__main__":
    sys.exit(main())
