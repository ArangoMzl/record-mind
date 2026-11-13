---
name: pendientes
argument-hint: "[números de tarea | todas]"
description: Resuelve los micro-pendientes del tablero record-mind del proyecto actual (correcciones que el usuario capturó con nota y captura de pantalla: tamaños de letra, imágenes que no cargan, detalles que faltan), consultando y alimentando la memoria de Engram. Úsalo cuando el usuario diga "pendientes", "/pendientes", "resuelve las tareas del tablero" o "mira lo que anoté".
---

# Pendientes de record-mind

El usuario va anotando correcciones pequeñas mientras trabaja contigo. Cada tarjeta trae su nota y,
casi siempre, una captura de pantalla. **La nota más la captura son la especificación**: mírala con
atención antes de tocar código.

## Primero: qué tareas trabajar

Llama a `list_tasks` de record-mind con `cwd` = tu carpeta de trabajo actual. Si no hay tareas,
dilo en una línea y termina. Después decide así, en este orden:

1. **El usuario pasó números** (`/pendientes 4 7`): solo esas.
2. **El usuario dijo «todas»** (`/pendientes todas`): todas las capturadas del proyecto.
3. **Hay tareas «★ en cola»**: el usuario las marcó en el tablero para esta pasada. Di en una línea
   cuáles vas a trabajar y trabaja **solo esas**, sin preguntar.
4. **Si no**, pregúntale con la herramienta `AskUserQuestion`, en una sola pregunta de selección
   múltiple (`multiSelect: true`):
   - **1 tarea:** opciones «Resolver #id» y «Ahora no».
   - **2 o 3 tareas:** una opción por tarea y otra «Todas».
   - **4 tareas:** una opción por tarea (marcarlas todas equivale a «todas»).
   - **5 o más:** primero escribe la lista numerada (`#id · título · prioridad · capturas`). Luego
     pregunta con opciones como «Todas (N)», «Solo prioridad alta (n)» (si hay) y «Las más
     recientes (3)». El usuario puede escribir los números en «Other».

   En cada opción por tarea: `label` = «#id título corto» (pocas palabras) y `description` = la
   nota resumida, más la prioridad y si trae captura. No empieces a editar hasta que responda.

Trabaja las elegidas por prioridad (urgente → alta → normal) y, dentro de cada prioridad, las más
antiguas primero.

## Flujo, una tarea a la vez

1. **Leer.** `get_task(id)`. Si la tarea **fue devuelta**, las notas de devolución son lo primero
   que hay que resolver: la vez anterior algo quedó mal.
2. **Tomar.** `claim_task(id)` justo antes de editar, **solo la tarea que vas a trabajar ahora**
   (no las tomes todas de golpe): el usuario ve en vivo cuál estás resolviendo.
3. **Recordar.** Revisa las «memorias de Engram posiblemente relacionadas» que trae la tarea. Si
   el arreglo no es trivial, busca además con `mem_search` de Engram (palabras clave del problema y
   del componente). Lee completas solo las útiles (`mem_get_observation`) y apunta sus ids.
4. **Arreglar.** Cambio mínimo y localizado. Nada de refactors ni mejoras que nadie pidió; si ves
   otro problema, regístralo con `create_task` en vez de arreglarlo de paso.
5. **Verificar.** Compila, corre las pruebas o el linter que apliquen, y si es visual y puedes,
   compruébalo en el navegador. No digas que verificaste algo que no verificaste.
6. **Aprender (solo si vale la pena).** Si descubriste algo no obvio y duradero (una causa raíz,
   una convención del proyecto, una trampa que se va a repetir), guárdalo en Engram con `mem_save`
   siguiendo su protocolo, y apunta el id. Ajustes triviales («subí el font-size») no van a Engram.
7. **Entregar.** `complete_task` con:
   - `summary`: qué cambiaste, cómo lo verificaste y qué no pudiste verificar, en 1–4 frases.
   - `files_changed`: rutas relativas.
   - `memories_consulted` / `memories_created`: los ids de Engram del paso 3 y del paso 6.

   La tarea queda «Para revisar»: **nunca** se marca como hecha desde aquí; la aprueba el usuario.

Si una tarea es ambigua o te bloqueas, deja la pregunta o el motivo con `add_comment` y **no** la
completes; pasa a la siguiente.

## Al terminar

Un resumen breve: una línea por tarea con `#id`, estado final y qué se hizo, y aparte las que
quedaron bloqueadas con su pregunta.
