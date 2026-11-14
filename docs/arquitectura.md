# Arquitectura

record-mind tiene tres piezas que comparten una sola base de datos:

| Pieza | Qué es | Quién la usa |
|---|---|---|
| **App de escritorio** (`app/`) | Tauri 2: tablero, captura rápida, bandeja, MCP embebido por HTTP | Tú |
| **CLI** (`crates/cli`) | Binario `record-mind`: comandos de terminal y MCP por stdio | Claude Code (y tú) |
| **Skill** (`claude/skills/pendientes`) | Instrucciones de `/pendientes` para Claude Code | Claude Code |

Por debajo, dos crates de biblioteca:

- **`rm-core`**: modelos, la base de tareas (SQLite con `rusqlite`) y el lector de Engram.
- **`rm-mcp`**: el servidor MCP (`rmcp`), con transporte stdio y Streamable HTTP.

```
                ┌────────────────────────── rm-core ──────────────────────────┐
                │  Store (lectura/escritura)          EngramReader (solo lectura)│
                │  ~/.record-mind/record-mind.db      ~/.engram/engram.db        │
                └──────▲──────────────────▲───────────────────▲────────────────┘
                       │                  │                   │
              ┌────────┴───────┐  ┌───────┴────────┐  ┌───────┴────────┐
              │ app (Tauri)    │  │ rm-mcp         │  │ cli            │
              │ comandos IPC   │  │ stdio · HTTP   │  │ add, list, …   │
              │ vigilante      │  └───────▲────────┘  └────────────────┘
              └────────────────┘          │
                                    Claude Code
```

## Modelo de datos

La base vive en `~/.record-mind/record-mind.db` (SQLite en modo WAL). Las capturas se guardan
como archivos en `~/.record-mind/attachments/<tarea>/<n>.png`, y la base solo guarda su ruta
relativa. Las migraciones se aplican al abrir y su número se guarda en `PRAGMA user_version`.

| Tabla | Contenido |
|---|---|
| `tasks` | proyecto, título, nota, estado, prioridad (0–2), sesión de origen, resolución, archivos tocados, `queued_at` (cola) |
| `attachments` | capturas de cada tarea (ruta relativa y tipo MIME) |
| `task_events` | historial: `created`, `claimed`, `comment`, `completed`, `approved`, `returned`, `moved`, `released`, `edited`, con su autor (`user`, `claude`, …) |
| `task_memories` | enlaces a memorias de Engram (`consulted` o `created`) por id o `sync_id` |

## Reglas del tablero

Las reglas viven en `rm-core` (`Store`), así que valen igual para la app, la CLI y el MCP:

| Transición | Cómo | Quién |
|---|---|---|
| Capturado → Claude trabajando | `claim_task` | Solo un agente (Claude) |
| Claude trabajando → Para revisar | `complete_task` con resumen | Solo un agente |
| Claude trabajando → Capturado | «Liberar» (`release`) | Persona, si la sesión de Claude se cortó |
| Para revisar → Hecho | «Aprobar» | Solo una persona |
| Para revisar / Hecho → Capturado | «Devolver con nota» (`reopen`) | Persona |
| Capturado ↔ Para revisar / Hecho | Arrastrar o «Mover a…» | Persona |

- Nadie puede **soltar** una tarea en «Claude trabajando» a mano, ni sacarla de ahí arrastrándola.
- **Editar** (`update`: título, nota, prioridad, proyecto) deja un evento `edited` con los campos
  cambiados. Mientras la tarea está en «Claude trabajando» solo se puede cambiar la prioridad.
  **Duplicar** crea una copia en Capturado con sus propias capturas.
- En el tablero, el **clic derecho** sobre una tarjeta abre su menú: abrir, editar, prioridad,
  cola, aprobar, mover, duplicar, copiar y eliminar (con doble confirmación). Con el menú abierto,
  las letras indicadas (`E`, `D`, `1`–`3`, `C`, `A`, `Supr`) disparan la acción.
- **Quién trabaja una tarea** se deduce del historial: un `claimed` abre el turno del agente y
  cualquier `moved`, `released`, `completed`, `returned` o `approved` lo cierra. La interfaz
  muestra ese agente, desde cuándo y su último mensaje.
- Un comentario del agente que contiene `?` y es lo último del historial se muestra como
  **pregunta** (recuadro ámbar) hasta que respondes; los demás comentarios son avances.
- **La cola**: las tareas marcadas «Para Claude» tienen `queued_at`. `/pendientes` sin argumentos
  trabaja solo esas, y `claim_task` saca la tarea de la cola.

## Actualización en vivo

La app abre una segunda conexión de solo lectura y consulta `PRAGMA data_version` cada 400 ms. Ese
valor cambia cada vez que **otra** conexión confirma una transacción, así que detecta por igual
las escrituras del MCP por stdio (otro proceso), de la CLI o de la propia app. Cuando cambia,
emite el evento `tasks:changed` y el tablero se recarga y compara el estado anterior para mostrar
avisos («Claude tomó #4», «#4 lista para revisar», «Claude pregunta en #4»).

## Integración con Engram

record-mind **nunca escribe** en la base de Engram, que mantiene índices, versiones y una cola de
sincronización. La abre con `SQLITE_OPEN_READ_ONLY` y la usa para tres cosas:

1. **Sesión activa (respaldo)**: solo cuando no se encuentran conversaciones de Claude Code (ver
   «Sesiones activas» más abajo). Es la sesión abierta con actividad más reciente (inicio,
   renovación de la concesión, último prompt o última memoria) en las últimas 12 horas. La
   concesión (`runtime_lease_expires_at`) vence 30 minutos después de renovarse, así que se usa
   menos 30 minutos: tal cual es una fecha futura y hacía ganar a cualquier sesión recién abierta.
2. **Proyecto de una carpeta**: `project_aliases` y luego la sesión cuya carpeta contiene el `cwd`
   (la más específica). Si no hay, el nombre de la carpeta en minúsculas.
3. **Memorias relacionadas**: búsqueda FTS5 (`observations_fts`, con tokenizador de trigramas) con
   las palabras útiles del título y la nota, en el proyecto o con alcance personal/global.

Las memorias nuevas las guarda **Claude** con `mem_save` durante `/pendientes`, a través de Engram,
y su id queda enlazado a la tarea con `complete_task`.

## Proyectos de Claude Code

El tablero y la captura no solo ofrecen los proyectos con tareas: también los que conoce Claude
Code, para poder capturar en uno que todavía no tiene ninguna (`rm_core::claude_code`).

- Se leen las carpetas de `projects` en `~/.claude.json` (o `$CLAUDE_CONFIG_DIR/.claude.json`).
  **Solo lectura**: nunca se toca la configuración de Claude.
- Se descartan la carpeta de usuario, `~/.claude` (sesiones de reserva) y todo lo que cuelga de
  `AppData` (temporales y espacios de trabajo de Claude Desktop), además de las carpetas que ya
  no existen.
- Cada carpeta se nombra con `resolve_project`, igual que Engram, el MCP y `/pendientes`.
- Primero van los proyectos con tareas (el más reciente primero) y después los demás, en orden
  alfabético y apagados en la barra lateral.

### Sesiones activas

Puede haber varias conversaciones abiertas a la vez, en proyectos distintos o en el mismo, así que
no hay una sola «sesión activa»: hay una lista (`claude_code::live_sessions`).

- Claude Code guarda cada conversación en `~/.claude/projects/<carpeta>/<sesión>.jsonl` (o bajo
  `$CLAUDE_CONFIG_DIR/projects`) y lo reescribe con cada mensaje. Cuenta como activa si se modificó
  en los últimos **30 minutos**. Su carpeta es el primer `cwd` de sus líneas y su id es el mismo
  que el de la sesión de Engram.
- Se ignoran los `.jsonl` de subagentes (dentro de la carpeta de la sesión) y las carpetas internas
  (mismo filtro que los proyectos).
- El tablero muestra un renglón por proyecto con su actividad más reciente y cuántas
  conversaciones tiene, y las vuelve a pedir cada 30 s (`live_sessions`): abrir o cerrar una
  conversación no escribe en la base de tareas. La captura marca todos esos proyectos y elige por
  defecto el de la conversación más reciente. Una tarea nueva queda enlazada a la conversación
  activa de su proyecto.

## La app de escritorio

- **Ventanas**: `main` (tablero) nace oculta y se muestra al arrancar, salvo si Windows la lanza con
  `--autostart`. `capture` no existe hasta que se usa el atajo por primera vez (`create: false`),
  para no gastar memoria de WebView2 mientras nadie la abre.
- **Bandeja**: cerrar el tablero lo esconde y solo «Salir» termina la app. El menú incluye
  «Iniciar con Windows» (`tauri-plugin-autostart`).
- **Atajos globales** (`shortcuts.rs`, `tauri-plugin-global-shortcut`): capturar pendiente
  (por defecto `CommandOrControl+Shift+Space`) y, opcional, recortar pantalla. Se guardan en
  `settings.json` en la carpeta de datos (`rm_core::settings`; si falta o está roto, valores por
  defecto). Al cambiar uno se prueba a registrarlo antes de guardar: si Windows u otra app ya lo
  tiene, se queda el anterior. Mientras el diálogo de Ajustes graba una combinación, todos los
  atajos se desregistran (`pause_shortcuts`) para que la tecla llegue al grabador y no dispare su
  acción; vuelven al terminar, al cancelar, al perder el foco o al cerrar el diálogo.
- **AltGr**: en distribuciones con AltGr (español, inglés internacional) Windows convierte
  Ctrl+Alt en AltGr y el navegador entrega `AltGraph` con `ctrlKey`/`altKey` en falso. El grabador
  lo cuenta como Ctrl+Alt, que es lo que recibe el atajo global.
- **Capturas**: se pegan con el evento `paste` del WebView y viajan por IPC en base64. Se sirven
  al tablero por `asset://`, con el permiso limitado en tiempo de ejecución a la carpeta de datos.
- **MCP embebido**: mientras la app está abierta, `rm-mcp` escucha por HTTP en
  `127.0.0.1:4545/mcp` (`RECORD_MIND_PORT` lo cambia), compartiendo el mismo `Store`.
- **WebView2** se lanza con `--disable-gpu`. El tablero no lo necesita y la memoria baja
  aproximadamente a la mitad.

## Datos y la virtualización de Claude Desktop

Claude Desktop es una app empaquetada con **MSIX**. Todo proceso lanzado desde sus sesiones de
Code (terminales, el MCP por stdio, instaladores) escribe lo nuevo de `%APPDATA%` y
`%LOCALAPPDATA%` dentro de `%LOCALAPPDATA%\Packages\Claude_<id>\LocalCache\…`. Desde dentro se ve
una vista combinada; fuera del paquete esos archivos **no existen**.

Por eso:

- Los datos van en **`~/.record-mind`**, porque la carpeta de usuario no se virtualiza (Engram usa
  `~/.engram` por lo mismo). Las versiones que guardaban en `%APPDATA%\record-mind` se migran solas
  al abrir: la base se copia con `VACUUM INTO` y las capturas archivo por archivo.
- La app se instala desde el Explorador. Un instalador lanzado desde Claude Desktop dejaría la app
  dentro del paquete, con el acceso directo apuntando a una ruta inexistente. Para lanzar algo
  fuera del paquete desde una sesión de Claude: `explorer.exe <programa>`.

## Variables de entorno

| Variable | Efecto |
|---|---|
| `RECORD_MIND_HOME` | Carpeta de datos (por defecto `~/.record-mind`) |
| `RECORD_MIND_PORT` | Puerto del MCP embebido en la app (por defecto `4545`) |
| `ENGRAM_DB` / `ENGRAM_DATA_DIR` | Base de Engram a leer (por defecto `~/.engram/engram.db`) |
| `CLAUDE_CONFIG_DIR` | Carpeta de la configuración de Claude Code de donde se leen los proyectos (por defecto `~`) |
| `RUST_LOG` | Nivel de logs (`tracing`), siempre por stderr |
