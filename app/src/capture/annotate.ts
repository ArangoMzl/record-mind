// Anotaciones sobre una captura: el modelo, su geometría y el render final a PNG.
// Todas las coordenadas van en píxeles de la imagen original, no de la pantalla: así el
// trazo se ve igual en el editor (SVG escalado) y en el PNG que se guarda (canvas a tamaño real).

export type Tool = "arrow" | "rect" | "pen" | "num";
export type Point = [number, number];

export interface Shape {
  tool: Tool;
  color: string;
  /** Grosor del trazo en píxeles de la imagen. */
  width: number;
  /** arrow/rect: [inicio, fin] · pen: el recorrido · num: [centro]. */
  pts: Point[];
  /** Solo en num: el número que muestra. */
  n?: number;
}

export const COLORS = [
  { value: "#ea1889", label: "Rosa" },
  { value: "#fbbf24", label: "Amarillo" },
  { value: "#38bdf8", label: "Cian" },
  { value: "#fafafa", label: "Blanco" },
] as const;

/** Grosor según el tamaño de la imagen: ~5 px en una captura de 1920. */
export const strokeFor = (w: number, h: number) => Math.min(12, Math.max(3, Math.round(Math.max(w, h) / 380)));

export const numRadius = (width: number) => width * 5;

/** Texto oscuro sobre los colores claros para que el número se lea. */
export const numInk = (color: string) => (color === "#fafafa" || color === "#fbbf24" ? "#09090b" : "#ffffff");

export interface ArrowGeom {
  /** Fin del tallo: un poco dentro de la punta para que no quede hueco. */
  shaft: [Point, Point];
  head: [Point, Point, Point];
}

export function arrowGeom([[x1, y1], [x2, y2]]: Point[], width: number): ArrowGeom {
  const len = Math.hypot(x2 - x1, y2 - y1) || 1;
  const ux = (x2 - x1) / len;
  const uy = (y2 - y1) / len;
  const headLen = Math.min(width * 5.5, len * 0.7);
  const half = headLen * 0.62;
  const bx = x2 - ux * headLen;
  const by = y2 - uy * headLen;
  return {
    shaft: [
      [x1, y1],
      [bx + ux * headLen * 0.3, by + uy * headLen * 0.3],
    ],
    head: [
      [x2, y2],
      [bx - uy * half, by + ux * half],
      [bx + uy * half, by - ux * half],
    ],
  };
}

export function rectGeom([[x1, y1], [x2, y2]]: Point[]) {
  return { x: Math.min(x1, x2), y: Math.min(y1, y2), w: Math.abs(x2 - x1), h: Math.abs(y2 - y1) };
}

/** Con Mayús: flechas en múltiplos de 45° y recuadros cuadrados. */
export function constrain(tool: Tool, [x1, y1]: Point, [x2, y2]: Point): Point {
  const dx = x2 - x1;
  const dy = y2 - y1;
  if (tool === "rect") {
    const s = Math.max(Math.abs(dx), Math.abs(dy));
    return [x1 + Math.sign(dx || 1) * s, y1 + Math.sign(dy || 1) * s];
  }
  if (tool === "arrow") {
    const len = Math.hypot(dx, dy);
    const a = Math.round(Math.atan2(dy, dx) / (Math.PI / 4)) * (Math.PI / 4);
    return [x1 + Math.cos(a) * len, y1 + Math.sin(a) * len];
  }
  return [x2, y2];
}

/** Un trazo demasiado pequeño suele ser un clic sin querer: no se guarda. */
export function isMeaningful(s: Shape, minPx: number): boolean {
  if (s.tool === "num") return true;
  if (s.tool === "pen") return s.pts.length > 1;
  const [[x1, y1], [x2, y2]] = s.pts;
  return Math.hypot(x2 - x1, y2 - y1) >= minPx;
}

export const nextNumber = (shapes: Shape[]) => shapes.filter((s) => s.tool === "num").length + 1;

function loadImage(src: string): Promise<HTMLImageElement> {
  return new Promise((resolve, reject) => {
    const img = new Image();
    img.onload = () => resolve(img);
    img.onerror = () => reject(new Error("no se pudo leer la imagen"));
    img.src = src;
  });
}

function drawShape(ctx: CanvasRenderingContext2D, s: Shape) {
  const w = s.width;
  ctx.save();
  ctx.strokeStyle = s.color;
  ctx.fillStyle = s.color;
  ctx.lineWidth = w;
  ctx.lineCap = "round";
  ctx.lineJoin = "round";
  // La misma sombra que el editor (drop-shadow en el SVG) para que se lea sobre cualquier fondo.
  ctx.shadowColor = "rgba(0,0,0,0.45)";
  ctx.shadowBlur = w * 0.9;
  ctx.shadowOffsetY = w * 0.25;

  if (s.tool === "rect") {
    const r = rectGeom(s.pts);
    ctx.beginPath();
    ctx.roundRect(r.x, r.y, r.w, r.h, w * 1.2);
    ctx.stroke();
  } else if (s.tool === "arrow") {
    const g = arrowGeom(s.pts, w);
    ctx.beginPath();
    ctx.moveTo(...g.shaft[0]);
    ctx.lineTo(...g.shaft[1]);
    ctx.stroke();
    ctx.beginPath();
    ctx.moveTo(...g.head[0]);
    ctx.lineTo(...g.head[1]);
    ctx.lineTo(...g.head[2]);
    ctx.closePath();
    ctx.fill();
  } else if (s.tool === "pen") {
    ctx.beginPath();
    ctx.moveTo(...s.pts[0]);
    for (const p of s.pts.slice(1)) ctx.lineTo(...p);
    ctx.stroke();
  } else {
    const [cx, cy] = s.pts[0];
    const r = numRadius(w);
    ctx.beginPath();
    ctx.arc(cx, cy, r, 0, Math.PI * 2);
    ctx.fill();
    ctx.shadowColor = "transparent";
    ctx.fillStyle = numInk(s.color);
    ctx.font = `700 ${Math.round(r * 1.15)}px "Geist Variable", system-ui, sans-serif`;
    ctx.textAlign = "center";
    ctx.textBaseline = "middle";
    ctx.fillText(String(s.n ?? 1), cx, cy + r * 0.06);
  }
  ctx.restore();
}

/** Pinta la imagen original con sus anotaciones y devuelve el PNG (data URL y base64). */
export async function renderAnnotated(src: string, shapes: Shape[]): Promise<{ url: string; data: string }> {
  const img = await loadImage(src);
  await document.fonts.ready;
  const canvas = document.createElement("canvas");
  canvas.width = img.naturalWidth;
  canvas.height = img.naturalHeight;
  const ctx = canvas.getContext("2d");
  if (!ctx) throw new Error("canvas no disponible");
  ctx.drawImage(img, 0, 0);
  for (const s of shapes) drawShape(ctx, s);
  const url = canvas.toDataURL("image/png");
  return { url, data: url.slice(url.indexOf(",") + 1) };
}
