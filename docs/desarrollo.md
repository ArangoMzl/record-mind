# Desarrollo

## Requisitos

- **Rust** estable (`rustup`). El workspace usa la edición 2024.
- **Visual Studio Build Tools** con «Desarrollo de escritorio con C++», que Rust necesita en Windows
  para enlazar.
- **Node.js 22+** para la interfaz.
- **WebView2**, que viene con Windows 11.

```bash
winget install --id Rustlang.Rustup -e
winget install --id Microsoft.VisualStudio.2022.BuildTools -e --override "--quiet --wait --add Microsoft.VisualStudio.Workload.VCTools --includeRecommended"
```

## Estructura del workspace

```
Cargo.toml               workspace: crates/* + app/src-tauri
crates/core   (rm-core)  Store, EngramReader, modelos, rutas, migraciones
crates/mcp    (rm-mcp)   servidor MCP y transportes
crates/cli    (record-mind) CLI y MCP por stdio
app/
  src-tauri/             app Tauri: lib.rs (ventanas, bandeja, atajo, MCP), commands.rs (IPC), watcher.rs
  src/board/             tablero: BoardApp, Column, TaskCardView, DetailDrawer, Menu, Sidebar, Toasts
  src/capture/           ventana de captura rápida
  src/lib/api.ts         tipos e invocaciones a los comandos de Rust
  design/icon.svg        fuente del icono
claude/skills/pendientes skill de Claude Code
site/                    página de presentación (HTML estático)
```

## Compilar y probar

```bash
cargo test                          # pruebas del núcleo (ciclo de tareas, reglas, cola, FTS)
cargo clippy --workspace            # sin avisos
cargo build --release -p record-mind  # CLI: target/release/record-mind.exe

cd app
npm install
npx tsc --noEmit                    # tipos de la interfaz
npm run tauri dev                   # app con recarga en caliente
npm run tauri build                 # instalador NSIS en target/release/bundle/nsis/
```

`npm run tauri dev` y la app instalada comparten identificador y puerto. Cierra la instalada
(«Salir» en la bandeja) antes de abrir la de desarrollo; si no, el complemento de instancia única
solo traerá al frente la instalada.

Para trabajar con datos de prueba sin tocar los tuyos:

```bash
RECORD_MIND_HOME=/tmp/rm-demo ENGRAM_DATA_DIR=/tmp/engram-demo npm run tauri dev
```

## Depurar la interfaz real

La interfaz depende de los comandos de Rust, así que en un navegador normal no funciona. Para
inspeccionar la app de verdad, lánzala con la depuración remota de WebView2:

```bash
WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS=--remote-debugging-port=9222 npm run tauri dev
```

Después abre `edge://inspect` o usa el protocolo CDP en `http://127.0.0.1:9222/json`.
`Page.captureScreenshot` sirve para capturas, `Runtime.evaluate` para ejecutar código e
`Input.dispatchMouseEvent` para arrastrar tarjetas (dnd-kit necesita eventos de ratón reales).

## Icono

La fuente es `app/design/icon.svg`. Para regenerar los tamaños:

```bash
# renderiza el SVG a 1024×1024 con fondo transparente y genera todos los tamaños
msedge --headless=new --default-background-color=00000000 --window-size=1024,1024 \
  --screenshot=app/design/icon.png app/design/icon.svg
cd app && npx tauri icon design/icon.png -o src-tauri/icons
```

## Publicar una versión

1. Sube la versión en `crates/*/Cargo.toml`, `app/src-tauri/Cargo.toml`,
   `app/src-tauri/tauri.conf.json` y `app/package.json`.
2. Compila: `cargo build --release -p record-mind` y `cd app && npm run tauri build`.
3. Etiqueta y publica:

   ```bash
   git tag -a vX.Y.Z -m "record-mind vX.Y.Z"
   git push origin vX.Y.Z
   gh release create vX.Y.Z \
     target/release/bundle/nsis/record-mind_X.Y.Z_x64-setup.exe \
     target/release/record-mind.exe \
     --title "record-mind vX.Y.Z — Novedad principal" --notes-file notas.md
   ```

Lo que publica la release se usa en otros sitios, así que conviene respetar el formato:

- **Título** `record-mind vX.Y.Z — Novedad principal`: lo que va tras la raya (— o –) es el texto
  del pill de la web. Que sea corto; en móvil se corta con «…». Sin raya, la web pone un texto
  genérico.
- **Asset `*_x64-setup.exe`**: la web enlaza ese instalador y la app lo descarga para
  actualizarse. GitHub calcula su SHA-256 (`digest`).
- **Bloque `## SHA-256`** en las notas, con líneas `<hash>  <archivo>`: respaldo de la app si
  falta el `digest`. La app lo oculta al mostrar las notas.

La web y el aviso de la app leen la release en vivo, así que no hay que tocar ni redesplegar la
web.

El instalador no está firmado, así que SmartScreen puede avisar al abrirlo: *Más información →
Ejecutar de todas formas*.

## Convenciones

- Textos de la interfaz, mensajes y comentarios en **español**.
- Las reglas de negocio van en `rm-core::Store`, no en la interfaz ni en el MCP.
- Nunca se escribe en la base de Engram.
- Logs con `tracing`, siempre por stderr (stdout es el canal del MCP por stdio).
