# Servidor MCP

El servidor MCP de record-mind le da a Claude Code las herramientas para trabajar el tablero. Se
implementa en `crates/mcp` con [`rmcp`](https://crates.io/crates/rmcp), el SDK oficial de MCP en
Rust.

## Transportes

| Modo | Comando | Cuándo |
|---|---|---|
| **stdio** | `record-mind mcp` | Recomendado para Claude Code: lo lanza solo y funciona aunque la app esté cerrada |
| **HTTP** | `record-mind serve --port 4545` | Streamable HTTP en `http://127.0.0.1:<puerto>/mcp` |
| **Embebido** | (automático) | La app abierta sirve HTTP en `127.0.0.1:4545/mcp` |

Todos escriben en la misma base, y el tablero se entera al instante.

```bash
# stdio, para todos los proyectos
claude mcp add -s user record-mind -- "C:/ruta/a/record-mind.exe" mcp

# o por HTTP contra la app abierta
claude mcp add -s user --transport http record-mind http://127.0.0.1:4545/mcp
```

## Herramientas

Las tareas se identifican por número (`id`). El proyecto se deduce de `cwd` igual que lo hace
Engram, o se indica con `project`. Los errores (tarea inexistente, transición no permitida) se
devuelven como resultado de la herramienta con `isError: true`, para que Claude los lea.

### `list_tasks`

Lista las tareas del tablero.

| Parámetro | Tipo | Descripción |
|---|---|---|
| `cwd` | string? | Carpeta de trabajo; se usa para deducir el proyecto |
| `project` | string? | Nombre exacto del proyecto (tiene prioridad sobre `cwd`) |
| `status` | string[]? | `inbox`, `in_progress`, `review`, `done`. Por defecto `inbox` e `in_progress` |
| `only_queued` | bool? | Solo las marcadas «Para Claude» en el tablero |

La respuesta indica qué tareas están **en cola** (`★ en cola`), su prioridad y cuántas capturas
tienen.

### `get_task`

| Parámetro | Tipo | Descripción |
|---|---|---|
| `id` | number | Número de la tarea |

Devuelve un bloque de texto con la nota, el estado, las **notas de devolución** (lo primero que hay
que resolver si la tarea volvió), la resolución anterior, el historial, las memorias de Engram
enlazadas y las posiblemente relacionadas. Después vienen **las capturas como imágenes**. Las de
más de 4.5 MB se omiten y se indica su ruta.

### `claim_task`

| Parámetro | Tipo | Descripción |
|---|---|---|
| `id` | number | Número de la tarea |

Pasa la tarea de «Capturado» a «Claude trabajando», la saca de la cola y registra quién la tomó.
Es la **única** forma de entrar en esa columna. Si la tarea ya estaba en curso, registra al nuevo
agente.

### `complete_task`

| Parámetro | Tipo | Descripción |
|---|---|---|
| `id` | number | Número de la tarea |
| `summary` | string | Qué se cambió, cómo se verificó y qué no se pudo verificar |
| `files_changed` | string[]? | Rutas relativas de los archivos tocados |
| `memories_created` | (number \| string)[]? | Memorias guardadas en Engram a raíz de la tarea (id o `sync_id`) |
| `memories_consulted` | (number \| string)[]? | Memorias de Engram que sirvieron |

Deja la tarea **«Para revisar»**. Nunca la marca como hecha: eso lo decide una persona.

### `add_comment`

| Parámetro | Tipo | Descripción |
|---|---|---|
| `id` | number | Número de la tarea |
| `message` | string | Avance, duda o bloqueo |

Si el mensaje lleva `?` y es lo último del historial, el tablero lo muestra como **pregunta**
hasta que el usuario responde.

### `create_task`

| Parámetro | Tipo | Descripción |
|---|---|---|
| `cwd` / `project` | string? | Proyecto (uno de los dos) |
| `note` | string | Qué está mal y dónde |
| `title` | string? | Título corto; por defecto, la primera línea de la nota |
| `priority` | number? | 0 normal, 1 alta, 2 urgente |

Para que Claude registre algo que detectó de paso y no es parte de la tarea actual.

### `find_patterns`

| Parámetro | Tipo | Descripción |
|---|---|---|
| `cwd` / `project` | string? | Proyecto (uno de los dos) |

Evidencia de errores que se repiten: las tareas que el usuario **devolvió** con sus notas, las
palabras que aparecen en varias tareas y las reglas ya sugeridas o aceptadas (para no repetirlas).
Termina con el criterio para sugerir: la misma causa en 2 o más tareas, o una devolución que deja
clara una convención; máximo 2 reglas por pasada.

### `suggest_rule`

| Parámetro | Tipo | Descripción |
|---|---|---|
| `cwd` / `project` | string? | Proyecto (uno de los dos) |
| `rule` | string | Regla para `CLAUDE.md`: imperativa, concreta y comprobable |
| `reason` | string | Qué error se repite y dónde se vio |
| `task_ids` | number[]? | Tareas que lo demuestran |

La regla queda **pendiente** en «Reglas sugeridas» del tablero. No acepta reglas vacías ni
repetidas en el proyecto. Solo cuando el usuario la acepta, la app la añade al `CLAUDE.md` del
proyecto, en la sección «Reglas aprendidas (record-mind)». Claude nunca edita `CLAUDE.md`.

## El flujo de `/pendientes`

El skill ([`claude/skills/pendientes/SKILL.md`](../claude/skills/pendientes/SKILL.md)) le indica a
Claude:

1. **Elegir**: números pasados como argumento, `todas`, las tareas en cola o, si no hay ninguna,
   preguntar con `AskUserQuestion` (selección múltiple).
2. Por cada tarea, una a la vez: `get_task` → `claim_task` → revisar memorias (y `mem_search` si
   no es trivial) → cambio mínimo → verificar → `mem_save` si aprendió algo duradero →
   `complete_task`.
3. Si una tarea es ambigua: `add_comment` con la pregunta, sin completarla.
4. **Errores que se repiten**: `find_patterns` y, si hay evidencia clara, `suggest_rule`.
   Con `/pendientes reglas` hace solo este paso.
5. Un resumen final con una línea por tarea y las reglas sugeridas.

## Probar el servidor a mano

```bash
record-mind serve --port 4546
# initialize → notifications/initialized → tools/call, con el encabezado mcp-session-id
curl -s -D h.txt -H 'content-type: application/json' -H 'accept: application/json, text/event-stream' \
  http://127.0.0.1:4546/mcp \
  -d '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"curl","version":"1"}}}'
```
