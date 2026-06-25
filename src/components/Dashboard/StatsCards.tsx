import { useState, useMemo, useCallback, useEffect, useRef } from "react";
import { FilePlus, FileEdit, FileX, ArrowRightLeft, ChevronRight, ArrowRight, Loader2, X } from "lucide-react";
import { diffLines as computeDiffLines } from "diff";
import type { ChangeStats, ChangeRecord } from "../../lib/tauri";
import { getSnapshotsForFile, getSnapshotContent, getFileContent } from "../../lib/tauri";

interface StatsCardsProps {
  stats: ChangeStats;
  onFilter: (type: string | null) => void;
  allChanges: ChangeRecord[];
}

/* ─── helpers ─── */
const shortPath = (p: string | null) => {
  if (!p) return "—";
  const segs = p.replace(/\\/g, "/").split("/").filter(Boolean);
  return segs.length > 2 ? `…/${segs.slice(-2).join("/")}` : segs.join("/");
};

const timeSince = (iso: string | null) => {
  if (!iso) return "";
  const diff = Date.now() - new Date(iso).getTime();
  if (diff < 60_000) return "just now";
  if (diff < 3_600_000) return `${Math.floor(diff / 60_000)}m ago`;
  if (diff < 86_400_000) return `${Math.floor(diff / 3_600_000)}h ago`;
  return `${Math.floor(diff / 86_400_000)}d ago`;
};

/* ─── inline diff component ─── */
function InlineDiff({ filePath }: { filePath: string }) {
  const [lines, setLines] = useState<{ type: "added" | "removed" | "unchanged"; content: string }[]>([]);
  const [loading, setLoading] = useState(true);
  const [isFirstScan, setIsFirstScan] = useState(false);

  useEffect(() => {
    let cancelled = false;
    getSnapshotsForFile(filePath)
      .then(async (snapshots) => {
        if (cancelled) return;
        if (snapshots.length < 2) {
          // First scan — show all content as "added" (green) since there's no previous version
          setIsFirstScan(true);
          try {
            const content = await getFileContent(filePath);
            if (cancelled) return;
            const preview = content.split("\n").slice(0, 12);
            setLines(preview.map((l) => ({ type: "added" as const, content: l })));
          } catch {
            setLines([{ type: "added", content: "(no content available)" }]);
          }
          return;
        }
        const [oldSnap, newSnap] = snapshots.slice(0, 2);
        const [oldContent, newContent] = await Promise.all([
          getSnapshotContent(oldSnap.id).catch(() => ""),
          getSnapshotContent(newSnap.id).catch(() => ""),
        ]);
        if (cancelled) return;
        const changes = computeDiffLines(oldContent || "", newContent || "");
        const result: { type: "added" | "removed" | "unchanged"; content: string }[] = [];
        for (const part of changes) {
          const partLines = part.value.split("\n");
          if (partLines[partLines.length - 1] === "") partLines.pop();
          for (const line of partLines) {
            if (part.added) result.push({ type: "added", content: line });
            else if (part.removed) result.push({ type: "removed", content: line });
            else result.push({ type: "unchanged", content: line });
          }
        }
        if (result.length > 20) {
          const changed = result.filter((r) => r.type !== "unchanged");
          const context = result.filter((r) => r.type === "unchanged").slice(0, 10);
          setLines([...context.slice(0, 5), ...changed.slice(0, 10), ...context.slice(5, 10)]);
        } else {
          setLines(result);
        }
      })
      .catch(() => {
        if (!cancelled) setLines([{ type: "unchanged", content: "(diff unavailable)" }]);
      })
      .finally(() => { if (!cancelled) setLoading(false); });
    return () => { cancelled = true; };
  }, [filePath]);

  if (loading) {
    return (
      <div className="sw-diff-loading">
        <Loader2 className="w-3 h-3 animate-spin" />
        <span>Loading diff…</span>
      </div>
    );
  }

  return (
    <div className="sw-diff">
      {isFirstScan && (
        <div className="sw-diff-header sw-diff-header--first">
          <span className="sw-diff-header-icon">📝</span>
          First scan — showing file content (no previous version)
        </div>
      )}
      {lines.map((line, i) => (
        <div key={i} className={`sw-diff-line sw-diff-line--${line.type}`}>
          <span className="sw-diff-ln">{i + 1}</span>
          <span className="sw-diff-prefix">{line.type === "added" ? "+" : line.type === "removed" ? "−" : " "}</span>
          <span className="sw-diff-text">{line.content || " "}</span>
        </div>
      ))}
    </div>
  );
}

/* ─── file row inside reveal panel ─── */
function RevealFileRow({ change, type }: { change: ChangeRecord; type: string }) {
  const [open, setOpen] = useState(false);
  const filename = change.filename || (change.file_path || "").split(/[/\\]/).pop() || "unknown";

  return (
    <div className="sw-r-file-wrap">
      <button
        className="sw-r-file"
        onClick={(e) => { e.stopPropagation(); setOpen((v) => !v); }}
      >
        <span className={`sw-r-dot sw-r-dot--${type.toLowerCase()}`} />
        <span className="sw-r-fname">{filename}</span>
        <span className="sw-r-fpath">{shortPath(change.file_path)}</span>
        {timeSince(change.detected_at) && <span className="sw-r-time">{timeSince(change.detected_at)}</span>}
        <ChevronRight className={`sw-r-chv ${open ? "open" : ""}`} />
      </button>
      {open && (
        <div className="sw-r-detail">
          {type === "MOVED" && (
            <>
              <div className="sw-r-dline">
                <span className="sw-r-dlbl sw-r-dlbl--amber">From</span>
                <span className="sw-r-dval sw-r-dval--faded">{change.previous_path || "—"}</span>
              </div>
              <div className="sw-r-darrow"><ArrowRight className="w-3 h-3 text-gray-300 dark:text-gray-600" /></div>
              <div className="sw-r-dline">
                <span className="sw-r-dlbl sw-r-dlbl--green">To</span>
                <span className="sw-r-dval">{change.new_path || change.file_path}</span>
              </div>
            </>
          )}
          {type === "MODIFIED" && (
            <div className="sw-r-diff-wrap">
              <InlineDiff filePath={change.file_path} />
            </div>
          )}
          {type === "DELETED" && (
            <div className="sw-r-dline">
              <span className="sw-r-dlbl sw-r-dlbl--red">Was at</span>
              <span className="sw-r-dval sw-r-dval--faded">{change.file_path}</span>
            </div>
          )}
          {type === "NEW" && (
            <div className="sw-r-dline">
              <span className="sw-r-dlbl sw-r-dlbl--green">Location</span>
              <span className="sw-r-dval">{change.file_path}</span>
            </div>
          )}
        </div>
      )}
    </div>
  );
}

/* ─── card config ─── */
const CARD_META = [
  { key: "NEW" as const, label: "New Files", Icon: FilePlus, color: "#059669", colorDark: "#34d399", bg: "#ecfdf5", bgDark: "rgba(52,211,153,0.12)", blob: "rgba(5,150,105,0.15)", blobDark: "rgba(52,211,153,0.18)" },
  { key: "MODIFIED" as const, label: "Modified", Icon: FileEdit, color: "#2563eb", colorDark: "#60a5fa", bg: "#eff6ff", bgDark: "rgba(96,165,250,0.12)", blob: "rgba(37,99,235,0.15)", blobDark: "rgba(96,165,250,0.18)" },
  { key: "DELETED" as const, label: "Deleted", Icon: FileX, color: "#dc2626", colorDark: "#f87171", bg: "#fef2f2", bgDark: "rgba(248,113,113,0.12)", blob: "rgba(220,38,38,0.15)", blobDark: "rgba(248,113,113,0.18)" },
  { key: "MOVED" as const, label: "Moved", Icon: ArrowRightLeft, color: "#d97706", colorDark: "#fbbf24", bg: "#fffbeb", bgDark: "rgba(251,191,36,0.12)", blob: "rgba(217,119,6,0.15)", blobDark: "rgba(251,191,36,0.18)" },
];

/* ─── main ─── */
export function StatsCards({ stats, onFilter, allChanges }: StatsCardsProps) {
  const [activeType, setActiveType] = useState<string | null>(null);
  const [closing, setClosing] = useState(false);
  const timerRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  const clearTimer = useCallback(() => {
    if (timerRef.current !== null) {
      clearTimeout(timerRef.current);
      timerRef.current = null;
    }
  }, []);

  // Cleanup timers on unmount
  useEffect(() => () => clearTimer(), [clearTimer]);

  const byType = useMemo(() => {
    const map: Record<string, ChangeRecord[]> = { NEW: [], MODIFIED: [], DELETED: [], MOVED: [] };
    for (const c of allChanges) if (map[c.change_type]) map[c.change_type].push(c);
    return map;
  }, [allChanges]);

  const counts = useMemo(() => ({
    NEW: stats.new_count,
    MODIFIED: stats.modified_count,
    DELETED: stats.deleted_count,
    MOVED: stats.moved_count,
  }), [stats]);

  const handleClick = useCallback((key: string) => {
    clearTimer();
    setActiveType((prev) => {
      if (prev === key) {
        // Close
        setClosing(true);
        timerRef.current = setTimeout(() => {
          setActiveType(null);
          setClosing(false);
          onFilter(null);
        }, 320);
        return prev; // keep current during close animation
      }
      // Open or switch
      if (prev !== null) {
        setClosing(true);
        timerRef.current = setTimeout(() => {
          setClosing(false);
          setActiveType(key);
          onFilter(key);
        }, 200);
        return prev; // keep current during switch animation
      }
      onFilter(key);
      return key;
    });
  }, [clearTimer, onFilter]);

  const handleClose = useCallback(() => {
    clearTimer();
    setClosing(true);
    timerRef.current = setTimeout(() => {
      setActiveType(null);
      setClosing(false);
      onFilter(null);
    }, 320);
  }, [clearTimer, onFilter]);

  const activeMeta = activeType ? CARD_META.find((m) => m.key === activeType) : null;

  return (
    <>
      <style>{`
        /* ── card grid ── */
        .sw-grid {
          display: grid;
          grid-template-columns: repeat(4, 1fr);
          gap: 14px;
        }
        @media (max-width: 1024px) { .sw-grid { grid-template-columns: repeat(2, 1fr); } }

        /* ── card shell (compact, no inline expansion) ── */
        .sw-card {
          position: relative;
          border-radius: 14px;
          background: white;
          border: 1px solid #e5e7eb;
          cursor: pointer;
          overflow: hidden;
          transition: border-color 0.3s,
                      box-shadow 0.35s cubic-bezier(.34,1.56,.64,1),
                      transform 0.3s cubic-bezier(.34,1.56,.64,1);
          padding: 18px 20px;
          display: flex;
          align-items: center;
          gap: 14px;
          /* reset button defaults */
          font: inherit;
          color: inherit;
          text-align: left;
          outline: none;
          -webkit-appearance: none;
        }
        .dark .sw-card {
          background: #1e293b;
          border-color: #334155;
        }
        .sw-card:hover {
          box-shadow: 0 4px 16px -4px var(--sw-ag);
          border-color: color-mix(in srgb, var(--sw-c) 20%, #e5e7eb);
        }
        .dark .sw-card:hover {
          box-shadow: 0 4px 16px -4px rgba(0,0,0,0.3);
          border-color: color-mix(in srgb, var(--sw-c) 20%, #334155);
        }
        .sw-card:active { transform: scale(0.985); }
        .sw-card.is-open {
          border-color: var(--sw-c);
          box-shadow: 0 8px 32px -4px var(--sw-ag);
          background: var(--sw-bg);
        }
        .dark .sw-card.is-open {
          box-shadow: 0 8px 32px -4px rgba(0,0,0,0.5);
          background: var(--sw-bg-dark);
        }

        /* ── liquid blob ── */
        .sw-blob {
          position: absolute;
          top: 12px;
          left: 16px;
          width: 44px;
          height: 44px;
          border-radius: 50%;
          background: var(--sw-blob);
          transform: scale(0);
          opacity: 0;
          transition: transform 0.55s cubic-bezier(.34,1.56,.64,1),
                      opacity 0.3s ease,
                      border-radius 0.4s ease;
          z-index: 0;
        }
        .sw-card.is-open .sw-blob {
          transform: scale(8);
          opacity: 0.45;
          border-radius: 12px;
        }
        .dark .sw-blob { background: var(--sw-blob-dark); }
        .dark .sw-card.is-open .sw-blob { opacity: 0.3; }

        /* ── header ── */
        .sw-head {
          position: relative;
          z-index: 1;
          display: flex;
          align-items: center;
          gap: 14px;
          flex: 1;
          min-width: 0;
        }
        .sw-ico {
          width: 44px;
          height: 44px;
          border-radius: 11px;
          display: grid;
          place-items: center;
          flex-shrink: 0;
          background: var(--sw-bg);
          color: var(--sw-c);
          position: relative;
          z-index: 1;
          transition: transform 0.35s cubic-bezier(.34,1.56,.64,1);
        }
        .dark .sw-ico {
          background: var(--sw-bg-dark);
          color: var(--sw-c-dark);
        }
        .sw-card.is-open .sw-ico { transform: scale(1.08) rotate(-3deg); }
        .sw-num {
          font-size: 28px;
          font-weight: 700;
          color: #111827;
          line-height: 1;
          font-variant-numeric: tabular-nums;
        }
        .dark .sw-num { color: #f1f5f9; }
        .sw-lbl { font-size: 12.5px; color: #9ca3af; margin-top: 2px; }
        .dark .sw-lbl { color: #64748b; }

        /* ── chevron ── */
        .sw-arrow {
          color: #d1d5db;
          transition: transform 0.3s cubic-bezier(.34,1.56,.64,1), color 0.2s;
          z-index: 1;
          flex-shrink: 0;
        }
        .sw-card:hover .sw-arrow { color: var(--sw-c); }
        .sw-card.is-open .sw-arrow {
          transform: rotate(90deg);
          color: var(--sw-c);
        }
        .dark .sw-arrow { color: #475569; }
        .dark .sw-card:hover .sw-arrow { color: var(--sw-c-dark); }
        .dark .sw-card.is-open .sw-arrow { color: var(--sw-c-dark); }

        /* ══════════════════════════════════════════════
           REVEAL PANEL — Clip Morph animation
           Disconnected from cards with gap
           ══════════════════════════════════════════════ */
        .sw-reveal {
          margin-top: 20px;
          border-radius: 14px;
          background: white;
          border: 1px solid #e5e7eb;
          overflow: hidden;
          /* clip-path: collapsed by default */
          clip-path: polygon(0 0, 100% 0, 100% 0, 0 0);
        }
        .dark .sw-reveal {
          background: #1e293b;
          border-color: #334155;
        }
        .sw-reveal.is-open {
          clip-path: polygon(0 0, 100% 0, 100% 100%, 0 100%);
          animation: swClipIn 0.45s cubic-bezier(.4,0,.2,1) forwards;
        }
        .sw-reveal.is-closing {
          clip-path: polygon(0 0, 100% 0, 100% 100%, 0 100%);
          animation: swClipOut 0.3s cubic-bezier(.6,0,1,.6) forwards;
        }
        @keyframes swClipIn {
          0% { clip-path: polygon(0 0, 100% 0, 100% 0, 0 0); }
          100% { clip-path: polygon(0 0, 100% 0, 100% 100%, 0 100%); }
        }
        @keyframes swClipOut {
          0% { clip-path: polygon(0 0, 100% 0, 100% 100%, 0 100%); }
          100% { clip-path: polygon(0 0, 100% 0, 100% 0, 0 0); }
        }

        /* ── reveal header ── */
        .sw-r-header {
          display: flex;
          align-items: center;
          gap: 10px;
          padding: 18px 24px 14px;
        }
        .sw-r-badge {
          font-size: 11px;
          font-weight: 600;
          color: white;
          padding: 3px 10px;
          border-radius: 999px;
          background: var(--sw-c);
        }
        .dark .sw-r-badge { color: #0f172a; }
        .sw-r-title {
          font-size: 14px;
          font-weight: 600;
          color: #374151;
        }
        .dark .sw-r-title { color: #e2e8f0; }
        .sw-r-close {
          margin-left: auto;
          display: flex;
          align-items: center;
          gap: 4px;
          font-size: 11px;
          color: #9ca3af;
          cursor: pointer;
          border: none;
          background: none;
          font-family: inherit;
          padding: 4px 8px;
          border-radius: 6px;
          transition: all 0.15s;
        }
        .sw-r-close:hover { color: #374151; background: #f3f4f6; }
        .dark .sw-r-close:hover { color: #e2e8f0; background: #334155; }

        /* ── file rows inside reveal ── */
        .sw-r-file-list {
          padding: 0 12px 12px;
          max-height: 320px;
          overflow-y: auto;
          scrollbar-width: thin;
          scrollbar-color: #e5e7eb transparent;
        }
        .dark .sw-r-file-list { scrollbar-color: #475569 transparent; }
        .sw-r-file-list::-webkit-scrollbar { width: 4px; }
        .sw-r-file-list::-webkit-scrollbar-track { background: transparent; }
        .sw-r-file-list::-webkit-scrollbar-thumb { background: #d1d5db; border-radius: 2px; }
        .dark .sw-r-file-list::-webkit-scrollbar-thumb { background: #475569; }

        .sw-r-file {
          width: 100%;
          display: flex;
          align-items: center;
          gap: 10px;
          padding: 10px 14px;
          border-radius: 10px;
          border: none;
          background: transparent;
          cursor: pointer;
          font-family: inherit;
          text-align: left;
          transition: background 0.15s;
          /* stagger animation: hidden by default */
          opacity: 0;
          transform: translateY(8px);
        }
        .sw-r-file:hover { background: #f9fafb; }
        .dark .sw-r-file:hover { background: rgba(255,255,255,0.04); }

        /* stagger delay for reveal open */
        .sw-reveal.is-open .sw-r-file { animation: swFadeUp 0.28s cubic-bezier(.34,1.2,.64,1) forwards; }
        .sw-reveal.is-open .sw-r-file:nth-child(1) { animation-delay: 0.06s; }
        .sw-reveal.is-open .sw-r-file:nth-child(2) { animation-delay: 0.10s; }
        .sw-reveal.is-open .sw-r-file:nth-child(3) { animation-delay: 0.14s; }
        .sw-reveal.is-open .sw-r-file:nth-child(4) { animation-delay: 0.18s; }
        .sw-reveal.is-open .sw-r-file:nth-child(5) { animation-delay: 0.22s; }
        .sw-reveal.is-open .sw-r-file:nth-child(6) { animation-delay: 0.26s; }
        .sw-reveal.is-open .sw-r-file:nth-child(7) { animation-delay: 0.30s; }
        .sw-reveal.is-open .sw-r-file:nth-child(8) { animation-delay: 0.34s; }
        @keyframes swFadeUp {
          from { opacity: 0; transform: translateY(8px); }
          to { opacity: 1; transform: translateY(0); }
        }

        /* switching: no stagger */
        .sw-reveal.switching .sw-r-file { opacity: 1; transform: none; animation: none; }

        .sw-r-dot {
          width: 7px;
          height: 7px;
          border-radius: 50%;
          flex-shrink: 0;
          background: var(--sw-c);
        }
        .dark .sw-r-dot { background: var(--sw-c-dark); }
        .sw-r-fname {
          flex: 1;
          font-size: 12.5px;
          font-weight: 500;
          color: #111827;
          white-space: nowrap;
          overflow: hidden;
          text-overflow: ellipsis;
        }
        .dark .sw-r-fname { color: #e2e8f0; }
        .sw-r-fpath {
          font-size: 11px;
          color: #9ca3af;
          white-space: nowrap;
          max-width: 180px;
          overflow: hidden;
          text-overflow: ellipsis;
          flex-shrink: 0;
        }
        .dark .sw-r-fpath { color: #64748b; }
        .sw-r-time {
          font-size: 10px;
          color: #9ca3af;
          white-space: nowrap;
          flex-shrink: 0;
        }
        .dark .sw-r-time { color: #64748b; }
        .sw-r-chv {
          width: 14px;
          height: 14px;
          color: #d1d5db;
          flex-shrink: 0;
          transition: transform 0.25s cubic-bezier(.34,1.56,.64,1);
        }
        .sw-r-chv.open { transform: rotate(90deg); color: var(--sw-c); }
        .dark .sw-r-chv { color: #475569; }
        .dark .sw-r-chv.open { color: var(--sw-c-dark); }

        /* ── detail panel (per file) ── */
        .sw-r-detail {
          padding: 6px 14px 10px 31px;
          border-top: 1px solid #f3f4f6;
          animation: swDetailPop 0.25s cubic-bezier(.34,1.56,.64,1);
        }
        .dark .sw-r-detail { border-top-color: #334155; }
        @keyframes swDetailPop {
          from { opacity: 0; transform: translateY(-6px); }
          to { opacity: 1; transform: translateY(0); }
        }

        .sw-r-dline { display: flex; align-items: baseline; gap: 8px; margin-bottom: 2px; }
        .sw-r-darrow { padding-left: 16px; line-height: 1; }
        .sw-r-dlbl {
          font-size: 9px;
          font-weight: 600;
          text-transform: uppercase;
          letter-spacing: 0.05em;
          flex-shrink: 0;
          min-width: 40px;
        }
        .sw-r-dlbl--amber { color: #d97706; }
        .sw-r-dlbl--green { color: #059669; }
        .sw-r-dlbl--blue  { color: #2563eb; }
        .sw-r-dlbl--red   { color: #dc2626; }
        .dark .sw-r-dlbl--amber { color: #fbbf24; }
        .dark .sw-r-dlbl--green { color: #34d399; }
        .dark .sw-r-dlbl--blue  { color: #60a5fa; }
        .dark .sw-r-dlbl--red   { color: #f87171; }
        .sw-r-dval {
          font-size: 11px;
          color: #374151;
          word-break: break-all;
          line-height: 1.5;
        }
        .dark .sw-r-dval { color: #94a3b8; }
        .sw-r-dval--faded { color: #9ca3af; text-decoration: line-through; }
        .dark .sw-r-dval--faded { color: #64748b; }

        /* ── inline diff inside reveal ── */
        .sw-r-diff-wrap {
          padding: 0;
          max-height: 180px;
          overflow-y: auto;
          border-radius: 6px;
          border: 1px solid #e5e7eb;
          font-family: 'SF Mono', 'Cascadia Code', 'Consolas', monospace;
          font-size: 11px;
          line-height: 1.6;
          scrollbar-width: thin;
        }
        .dark .sw-r-diff-wrap { border-color: #334155; }
        .sw-diff {
          display: flex;
          flex-direction: column;
        }
        .sw-diff-line {
          display: flex;
          padding: 0 8px;
          white-space: pre-wrap;
          word-break: break-all;
        }
        .sw-diff-line--added { background: #dcfce7; color: #166534; }
        .dark .sw-diff-line--added { background: rgba(34,197,94,0.12); color: #4ade80; }
        .sw-diff-line--removed { background: #fee2e2; color: #991b1b; text-decoration: line-through; }
        .dark .sw-diff-line--removed { background: rgba(239,68,68,0.12); color: #f87171; text-decoration: line-through; }
        .sw-diff-line--unchanged { color: #9ca3af; }
        .dark .sw-diff-line--unchanged { color: #64748b; }
        .sw-diff-prefix {
          width: 14px;
          text-align: center;
          flex-shrink: 0;
          user-select: none;
          opacity: 0.6;
        }
        .sw-diff-text { flex: 1; min-width: 0; }
        .sw-diff-ln {
          width: 28px;
          text-align: right;
          flex-shrink: 0;
          color: #d1d5db;
          padding-right: 8px;
          user-select: none;
          font-size: 10px;
        }
        .dark .sw-diff-ln { color: #475569; }
        .sw-diff-header {
          padding: 6px 10px;
          font-size: 11px;
          color: #6b7280;
          display: flex;
          align-items: center;
          gap: 6px;
          border-bottom: 1px solid #e5e7eb;
        }
        .dark .sw-diff-header { border-bottom-color: #334155; color: #94a3b8; }
        .sw-diff-header--first {
          background: #f0fdf4;
          color: #166534;
        }
        .dark .sw-diff-header--first {
          background: rgba(34,197,94,0.08);
          color: #4ade80;
        }
        .sw-diff-header-icon { font-size: 12px; }
        .sw-diff-loading {
          display: flex;
          align-items: center;
          gap: 6px;
          padding: 6px 8px;
          font-size: 11px;
          color: #9ca3af;
        }
        .dark .sw-diff-loading { color: #64748b; }

        .sw-r-empty {
          padding: 20px;
          text-align: center;
          font-size: 12px;
          color: #9ca3af;
        }
        .dark .sw-r-empty { color: #64748b; }
      `}</style>

      {/* ── Card Grid ── */}
      <div className="sw-grid">
        {CARD_META.map(({ key, label, Icon, color, colorDark, bg, bgDark, blob, blobDark }) => {
          const isOpen = activeType === key;
          const count = counts[key];
          return (
            <button
              key={key}
              className={`sw-card ${isOpen ? "is-open" : ""}`}
              aria-label={`${label}, ${count} found${isOpen ? ", expanded" : ""}`}
              aria-expanded={isOpen}
              style={{
                ["--sw-c" as string]: color,
                ["--sw-c-dark" as string]: colorDark,
                ["--sw-ag" as string]: `${color}33`,
                ["--sw-bg" as string]: bg,
                ["--sw-bg-dark" as string]: bgDark,
                ["--sw-blob" as string]: blob,
                ["--sw-blob-dark" as string]: blobDark,
              } as React.CSSProperties}
              onClick={() => handleClick(key)}
            >
              <div className="sw-blob" />
              <div className="sw-head">
                <div className="sw-ico"><Icon className="w-[20px] h-[20px]" /></div>
                <div>
                  <div className="sw-num">{count}</div>
                  <div className="sw-lbl">{label}</div>
                </div>
              </div>
              <ChevronRight className="sw-arrow w-4 h-4" />
            </button>
          );
        })}
      </div>

      {/* ── Reveal Panel (disconnected, below cards) ── */}
      {activeType && activeMeta && (
        <div
          className={`sw-reveal ${closing ? "is-closing" : "is-open"} ${activeType ? "" : ""}`}
          style={{
            ["--sw-c" as string]: activeMeta.color,
            ["--sw-c-dark" as string]: activeMeta.colorDark,
          } as React.CSSProperties}
        >
          <div className="sw-r-header">
            <span className="sw-r-badge">{activeMeta.label}</span>
            <span className="sw-r-title">{byType[activeType].length} files</span>
            <button className="sw-r-close" onClick={handleClose}>
              <X className="w-3 h-3" />
              Close
            </button>
          </div>
          {byType[activeType].length > 0 ? (
            <div className="sw-r-file-list">
              {byType[activeType].map((c) => (
                <RevealFileRow key={c.id} change={c} type={activeType} />
              ))}
            </div>
          ) : (
            <div className="sw-r-empty">No files</div>
          )}
        </div>
      )}
    </>
  );
}
