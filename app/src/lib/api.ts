import { convertFileSrc, invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

export type Status = "inbox" | "in_progress" | "review" | "done";
export type MemoryRelation = "consulted" | "created";

export interface Attachment {
  id: number;
  task_id: number;
  path: string;
  mime: string;
  created_at: string;
}

export interface Task {
  id: number;
  project: string;
  title: string;
  note: string;
  status: Status;
  priority: number;
  session_id: string | null;
  resolution: string | null;
  files_changed: string[];
  attachments: Attachment[];
  created_at: string;
  updated_at: string;
  /** Marcada para la próxima pasada de /pendientes. */
  queued: boolean;
}

export interface Worker {
  actor: string;
  since: string;
  last_activity: string;
  last_message: string | null;
}

export interface TaskCard extends Task {
  /** Quién la trabaja de verdad; null en «Claude trabajando» = nadie la tomó. */
  worker: Worker | null;
  memory_count: number;
  memories_created: number;
  blocked: string | null;
  returns: number;
  /** Desde cuándo está en su columna (UTC). Editarla o comentarla no la cambia. */
  status_since: string;
}

export type Period = "all" | "today" | "yesterday" | "week";

export const PERIODS: { id: Period; label: string; hint: string; phrase: string }[] = [
  { id: "all", label: "Todo", hint: "Todas las tareas", phrase: "" },
  { id: "today", label: "Hoy", hint: "Lo que llegó a cada columna hoy", phrase: "hoy" },
  { id: "yesterday", label: "Ayer", hint: "Lo que llegó a cada columna ayer", phrase: "ayer" },
  { id: "week", label: "7 días", hint: "Lo que llegó a cada columna en los últimos 7 días (hoy incluido)", phrase: "en 7 días" },
];

/**
 * ¿Llegó la tarea a su columna dentro del periodo? Los días son de tu hora local, no UTC:
 * algo aprobado a las 8 de la noche cuenta para hoy.
 */
export function inPeriod(t: TaskCard, period: Period, now = new Date()): boolean {
  if (period === "all") return true;
  const y = now.getFullYear();
  const m = now.getMonth();
  const d = now.getDate();
  // Con el constructor de Date (y no sumando 24 h) los cambios de horario no descuadran el día.
  const [from, to] =
    period === "today" ? [new Date(y, m, d), new Date(y, m, d + 1)]
    : period === "yesterday" ? [new Date(y, m, d - 1), new Date(y, m, d)]
    : [new Date(y, m, d - 6), new Date(y, m, d + 1)];
  const at = parseDate(t.status_since).getTime();
  return at >= from.getTime() && at < to.getTime();
}

export interface SessionInfo {
  id: string;
  project: string;
  directory: string;
  started_at: string;
  /** Última actividad de la conversación (UTC). */
  last_activity: string;
  /** Rama de git de su carpeta (un worktree puede estar en otra que el proyecto). */
  branch: string | null;
}

export interface ProjectSummary {
  name: string;
  counts: [number, number, number, number];
  directory: string | null;
  /** Rama de git actual (commit abreviado si el HEAD está separado); null si no es un repo. */
  branch: string | null;
}

export interface Board {
  tasks: TaskCard[];
  /** Con tareas primero; después los proyectos de Claude Code sin tareas (contadores a cero). */
  projects: ProjectSummary[];
  /** Reglas para CLAUDE.md que Claude sugirió y esperan tu decisión. */
  rules: RuleSuggestion[];
  /** Conversaciones con decisiones (pendientes o tomadas), la actualizada más recientemente primero. */
  decisions: DecisionFile[];
  /** Conversaciones de Claude Code con actividad reciente, la más reciente primero. */
  sessions: SessionInfo[];
  mcp_url: string | null;
  engram: boolean;
  shortcut: string;
}

export interface TaskEvent {
  id: number;
  task_id: number;
  kind: string;
  actor: string;
  message: string;
  created_at: string;
}

export interface Memory {
  id: number;
  sync_id: string | null;
  kind: string;
  title: string;
  snippet: string;
  project: string | null;
  topic_key: string | null;
  created_at: string;
}

export interface TaskDetail extends Task {
  worker: Worker | null;
  events: TaskEvent[];
  memories: { engram_ref: string; relation: MemoryRelation; created_at: string }[];
  linked: { reference: string; relation: MemoryRelation; memory: Memory | null }[];
  related: Memory[];
}

export interface CaptureContext {
  sessions: SessionInfo[];
  projects: string[];
}

/** Campos a cambiar; los que no van se dejan como están. */
export interface TaskEdit {
  title?: string;
  note?: string;
  priority?: number;
  project?: string;
}

export const PRIORITIES = [
  { value: 0, label: "Normal" },
  { value: 1, label: "Alta" },
  { value: 2, label: "Urgente" },
] as const;

export interface ImageInput {
  data: string;
  ext: string;
}

export interface SnipSource {
  path: string;
  width: number;
  height: number;
}

export interface SnipRect {
  x: number;
  y: number;
  width: number;
  height: number;
}

export interface RuleSuggestion {
  id: number;
  project: string;
  rule: string;
  /** Qué error se repite y dónde se vio. */
  reason: string;
  task_ids: number[];
  status: "pending" | "accepted" | "rejected";
  created_at: string;
  decided_at: string | null;
}

export interface RuleDecision {
  rule: RuleSuggestion;
  /** CLAUDE.md donde quedó escrita (al aceptar). */
  path: string | null;
  /** false si ya estaba en el archivo. */
  added: boolean;
}

export interface Decision {
  /** `<proyecto>/<archivo>#<n>`. */
  id: string;
  question: string;
  context: string;
  recommendation: string | null;
  /** Cuándo se registró (UTC). */
  created_at: string | null;
  resolved: boolean;
  resolution: string | null;
  resolved_at: string | null;
  /** «tablero» o «Claude». */
  resolved_by: string | null;
}

/** Un .md de decisiones: una conversación de Claude Code. */
export interface DecisionFile {
  key: string;
  path: string;
  project: string;
  session_id: string | null;
  title: string;
  summary: string;
  created_at: string;
  updated_at: string;
  decisions: Decision[];
}

export type ShortcutAction = "capture" | "snip";

export interface ShortcutsView {
  capture: string;
  snip: string | null;
  default_capture: string;
  /** Por qué no se pudo registrar al arrancar (otra app ya lo usa…). */
  capture_error: string | null;
  snip_error: string | null;
}

export interface ReleaseNotes {
  version: string;
  name: string;
  /** Cuerpo de la release en Markdown. */
  notes: string;
  url: string;
  published_at: string;
}

export interface UpdateInfo {
  current: string;
  latest: string;
  /** Versiones posteriores a la instalada, la más nueva primero (vacío si está al día). */
  releases: ReleaseNotes[];
  /** Instalador verificable de la más nueva; null si no hay o no trae SHA-256. */
  installer: { name: string; url: string; size: number; sha256: string } | null;
}

export interface UpdateProgress {
  downloaded: number;
  total: number;
}

export const api = {
  board: () => invoke<Board>("board"),
  /** Conversaciones de Claude Code con actividad reciente (consulta ligera). */
  sessions: () => invoke<SessionInfo[]>("live_sessions"),
  /** Rama actual de cada carpeta, en el mismo orden. */
  gitBranches: (directories: string[]) => invoke<(string | null)[]>("git_branches", { directories }),
  detail: (id: number) => invoke<TaskDetail>("task_detail", { id }),
  captureContext: () => invoke<CaptureContext>("capture_context"),
  create: (input: { project: string; note: string; title?: string; priority: number; images: ImageInput[] }) =>
    invoke<Task>("create_task", { input }),
  move: (id: number, status: Status) => invoke<Task>("move_task", { id, status }),
  release: (id: number, reason: string) => invoke<Task>("release_task", { id, reason }),
  queue: (ids: number[], queued: boolean) => invoke<void>("queue_tasks", { ids, queued }),
  clearQueue: (project: string | null) => invoke<number>("clear_queue", { project }),
  approve: (id: number) => invoke<Task>("approve_task", { id }),
  reopen: (id: number, note: string) => invoke<Task>("reopen_task", { id, note }),
  comment: (id: number, message: string) => invoke<void>("comment_task", { id, message }),
  update: (id: number, edit: TaskEdit) => invoke<Task>("update_task", { id, edit }),
  duplicate: (id: number) => invoke<Task>("duplicate_task", { id }),
  remove: (id: number) => invoke<void>("delete_task", { id }),
  hideCapture: () => invoke<void>("hide_capture"),
  /** Aceptar escribe la regla (quizá retocada) en el CLAUDE.md del proyecto; descartar solo la marca. */
  decideRule: (id: number, accept: boolean, rule?: string) => invoke<RuleDecision>("decide_rule", { id, accept, rule: rule ?? null }),
  /** Marca una decisión como tomada (con lo que se decidió) o la descarta. */
  resolveDecision: (id: string, resolution: string | null, discard = false) =>
    invoke<Decision>("resolve_decision", { id, resolution, discard }),
  reopenDecision: (id: string) => invoke<Decision>("reopen_decision", { id }),
  /** Abre el .md de la conversación con el programa predeterminado. */
  openDecisionFile: (id: string) => invoke<void>("open_decision_file", { id }),
  /** Abre el dictado de Windows (Win+H) sobre el campo con el foco. */
  startDictation: () => invoke<void>("start_dictation"),
  getShortcuts: () => invoke<ShortcutsView>("get_shortcuts"),
  /** null quita el atajo (solo el de recortar). Falla con un mensaje legible si no se puede usar. */
  setShortcut: (action: ShortcutAction, shortcut: string | null) => invoke<ShortcutsView>("set_shortcut", { action, shortcut }),
  /** Mientras se graba un atajo nuevo, los globales se apagan para que la tecla llegue al grabador. */
  pauseShortcuts: (paused: boolean) => invoke<void>("pause_shortcuts", { paused }),
  /** Congela el monitor del ratón y abre la ventana de recorte. */
  startSnip: () => invoke<void>("start_snip"),
  snipSource: () => invoke<SnipSource | null>("snip_source"),
  snipReady: () => invoke<void>("snip_ready"),
  /** Con un área (píxeles físicos) la añade a la captura; con null cancela. */
  finishSnip: (rect: SnipRect | null) => invoke<void>("finish_snip", { rect }),
  /** Recortes listos que aún no se añadieron a la captura. */
  takeSnips: () => invoke<ImageInput[]>("take_snips"),
  /** Agranda (true) o devuelve a su tamaño (false) la ventana de captura. */
  resizeCapture: (editing: boolean) => invoke<void>("resize_capture", { editing }),
  openBoard: () => invoke<void>("open_board"),
  openCapture: () => invoke<void>("open_capture"),
  /** Consulta las releases de GitHub y las compara con la versión instalada. */
  checkUpdate: () => invoke<UpdateInfo>("check_update"),
  /** Descarga el instalador, comprueba su SHA-256, lo abre y cierra la app. */
  installUpdate: () => invoke<void>("install_update"),
  /** Abre un enlace https en el navegador (la webview no debe navegar fuera). */
  openUrl: (url: string) => invoke<void>("open_url", { url }),
};

export const onUpdateProgress = (cb: (p: UpdateProgress) => void): Promise<UnlistenFn> =>
  listen<UpdateProgress>("update:progress", (e) => cb(e.payload));

export const onTasksChanged = (cb: () => void): Promise<UnlistenFn> => listen("tasks:changed", cb);
export const onCaptureOpen = (cb: () => void): Promise<UnlistenFn> => listen("capture:open", cb);

export const assetUrl = (path: string) => convertFileSrc(path);

export const STATUSES: { id: Status; label: string; hint: string; color: string }[] = [
  { id: "inbox", label: "Capturado", hint: "Esperando a Claude", color: "var(--color-st-inbox)" },
  { id: "in_progress", label: "Claude trabajando", hint: "En curso ahora", color: "var(--color-st-progress)" },
  { id: "review", label: "Para revisar", hint: "Te toca a ti", color: "var(--color-st-review)" },
  { id: "done", label: "Hecho", hint: "Aprobado", color: "var(--color-st-done)" },
];

export const statusLabel = (s: Status) => STATUSES.find((x) => x.id === s)!.label;

/**
 * Lo único que se mueve a mano es Para revisar ↔ Hecho. Capturado y «Claude trabajando»
 * los mueve Claude; para devolver una tarea a Capturado está «Devolver con nota».
 */
export const canMove = (from: Status, to: Status) =>
  (from === "review" && to === "done") || (from === "done" && to === "review");

/** Si una tarjeta en este estado se puede arrastrar o mover a mano. */
export const isMovable = (s: Status) => s === "review" || s === "done";

/** Fechas de SQLite/Engram ("2026-11-13 21:36:48" o ISO) → Date en UTC. */
export function parseDate(s: string): Date {
  const iso = s.includes("T") ? s : s.replace(" ", "T");
  return new Date(/Z|[+-]\d\d:?\d\d$/.test(iso) ? iso : `${iso}Z`);
}

export function timeAgo(s: string): string {
  const sec = Math.max(0, (Date.now() - parseDate(s).getTime()) / 1000);
  if (sec < 45) return "ahora";
  const min = sec / 60;
  if (min < 60) return `hace ${Math.round(min)} min`;
  const h = min / 60;
  if (h < 24) return `hace ${Math.round(h)} h`;
  const d = h / 24;
  if (d < 7) return `hace ${Math.round(d)} d`;
  return parseDate(s).toLocaleDateString("es", { day: "numeric", month: "short" });
}

const KEY_LABEL: Record<string, string> = {
  CommandOrControl: "Ctrl",
  CmdOrCtrl: "Ctrl",
  Control: "Ctrl",
  Super: "Win",
  Meta: "Win",
  Shift: "⇧",
  Space: "Espacio",
  ArrowUp: "↑",
  ArrowDown: "↓",
  ArrowLeft: "←",
  ArrowRight: "→",
  Backquote: "`",
  Minus: "-",
  Equal: "=",
  Comma: ",",
  Period: ".",
  Slash: "/",
  Backslash: "\\",
  Semicolon: ";",
  Quote: "'",
  BracketLeft: "[",
  BracketRight: "]",
};

/** Acelerador de Tauri → teclas legibles: `CommandOrControl+Shift+KeyS` → [Ctrl, ⇧, S]. */
export function shortcutLabel(s: string): string[] {
  return s.split("+").map((k) => {
    if (KEY_LABEL[k]) return KEY_LABEL[k];
    if (/^Key[A-Z]$/.test(k)) return k.slice(3);
    if (/^Digitd$/.test(k)) return k.slice(5);
    if (/^Numpadd$/.test(k)) return "Num " + k.slice(6);
    return k;
  });
}

export const EVENT_LABEL: Record<string, string> = {
  created: "capturó",
  claimed: "tomó la tarea",
  completed: "la dejó para revisar",
  approved: "aprobó",
  returned: "la devolvió",
  comment: "comentó",
  moved: "la movió",
  released: "la liberó",
  edited: "editó",
};

export const actorName = (actor: string) => (actor === "user" ? "Tú" : actor === "claude" ? "Claude" : actor);
