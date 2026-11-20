---
name: decisiones
argument-hint: "[nada | revisar]"
description: Deja en el tablero de record-mind las decisiones que quedaron pendientes en esta conversación (lo que el usuario debe decidir y aún no decidió), con un resumen, y cierra las que ya se resolvieron. Las deudas técnicas las crea como capturas. Úsalo cuando el usuario diga "decisiones", "/decisiones", "qué quedó pendiente de decidir", "déjame las decisiones pendientes" o "pásame un md con lo pendiente".
---

# Decisiones pendientes de record-mind

En una conversación larga van quedando cosas que el usuario tiene que decidir, pero la
conversación sigue y se olvidan. Tu trabajo es dejarlas registradas en record-mind para que las
vea en la pestaña **«Decisiones»** del tablero. El archivo lo escribe record-mind: **no escribas tú
el .md**.

## 1. Lo que ya hay

Llama a `list_decisions` de record-mind con `cwd` = tu carpeta de trabajo actual. Te da las
decisiones abiertas del proyecto, de esta y de otras conversaciones, cada una con su `id`.

## 2. Cerrar las que ya se resolvieron

Revisa la conversación completa (y su resumen, si se compactó). Por cada decisión abierta de la
lista que **quedó resuelta aquí** (el usuario eligió, o se hizo algo que la deja zanjada), llama a
`resolve_decision` con su `id` y lo que se decidió en una frase. Si no estás seguro de que se
resolvió, déjala abierta.

## 3. Encontrar las nuevas

Busca en la conversación lo que **el usuario tiene que decidir y aún no decidió**:

- Preguntas que le hiciste y quedaron sin respuesta porque la conversación siguió.
- Cosas que elegiste tú por defecto y conviene que confirme («usé SQLite, confírmalo»).
- Opciones que se plantearon y se dejaron para después.

**No** son decisiones:

- Lo que ya se decidió, aunque sea de pasada.
- Tareas por hacer sin nada que elegir, y deudas técnicas («esto quedó a medias», «falta probar
  X»): créalas con `create_task` (una por deuda, nota clara de qué falta y dónde) para que queden
  como capturas en el tablero. Antes mira con `list_tasks` que no existan ya.
- Detalles triviales que no cambian el resultado.

Escribe cada decisión así:

- `question`: una pregunta corta y concreta. «¿Guardamos los md en ~/.record-mind o en el repo?»,
  no «Almacenamiento».
- `context`: por qué hay que decidirlo y qué opciones hay, en 1–3 frases. Que se entienda sin
  releer la conversación.
- `recommendation`: lo que recomiendas tú, en una frase, si tienes una.

## 4. Registrar

Si hay decisiones nuevas, llama **una vez** a `record_decisions` con:

- `cwd`: tu carpeta de trabajo.
- `title`: de qué trató la conversación, en pocas palabras.
- `summary`: qué se hizo y qué quedó a medias, en 2–5 frases.
- `decisions`: la lista del paso 3.

Si esta conversación ya tenía archivo, record-mind lo actualiza y no repite preguntas, así que
puedes correr `/decisiones` varias veces en la misma conversación.

Si no hay decisiones nuevas, no llames a `record_decisions`.

## Resumen final

En pocas líneas: cuántas decisiones abiertas quedaron (con sus preguntas), cuáles cerraste y qué
capturas de deuda técnica creaste. Recuérdale que las ve en «Decisiones» del tablero.

Con `/decisiones revisar` haz solo los pasos 1 y 2: cerrar las resueltas, sin registrar nuevas.
