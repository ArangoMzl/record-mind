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
}

export interface SessionInfo {
  id: string;
  project: string;
  directory: string;
  started_at: string;
}

export interface Board {
  tasks: TaskCard[];
  projects: { name: string; counts: [number, number, number, number] }[];
  active: SessionInfo | null;
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
  active: SessionInfo | null;
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

export const api = {
  board: () => invoke<Board>("board"),
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
  openBoard: () => invoke<void>("open_board"),
  openCapture: () => invoke<void>("open_capture"),
};

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

export function shortcutLabel(s: string): string[] {
  return s
    .replace("CommandOrControl", "Ctrl")
    .split("+")
    .map((k) => (k === "Shift" ? "⇧" : k === "Space" ? "Espacio" : k));
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
