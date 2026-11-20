import { GitBranch } from "lucide-react";

/** Rama de git de un proyecto o de una sesión (o el commit abreviado si el HEAD está separado). */
export function BranchTag({ branch, size = "sm", className = "" }: { branch: string; size?: "sm" | "md"; className?: string }) {
  const md = size === "md";
  return (
    <span
      title={`Rama ${branch}`}
      className={`inline-flex min-w-0 items-center gap-1 font-mono text-ink-3 ${md ? "text-[11.5px]" : "text-[10.5px]"} ${className}`}
    >
      <GitBranch size={md ? 12 : 10} className="shrink-0" />
      <span className="truncate">{branch}</span>
    </span>
  );
}
