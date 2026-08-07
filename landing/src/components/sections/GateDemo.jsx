import { useEffect, useRef, useState } from 'react'
import { useInView } from '../ui/Reveal'

/* One terminal, one destructive command, and the product's whole argument in
   five lines: the agent is stopped, a human says yes, Aegis takes a snapshot
   before the command runs, and one command puts everything back.

   The order matters. The safety net is revealed *after* the decision, because
   that is the order Aegis works in — Danger asks first, then snapshots on
   approval (README, decision flow). */
const RISKY_CMD = 'rm -rf ~/project/src'
const UNDO_CMD = 'aegis rollback a4f1c9'

const SUMMARY =
  'A terminal session: the agent runs rm -rf ~/project/src. Aegis stops the command and asks for a decision. Allow is pressed; Aegis saves a snapshot before the command runs. A single command, aegis rollback a4f1c9, restores the deleted folder.'

const SNAPSHOT_LINE = 'snapshot saved before it ran'
const RESTORED_LINE = 'src/ restored — nothing was lost'

/* ── The cycle, as named beats ────────────────────────────────────────
   The run used to be a column of bare `sleep(2100)` calls, and it read as
   slow for a reason a uniform speed-up would not have fixed: three static
   holds were 60% of it, and the longest — a full quarter of the cycle — sat
   at the very end, after the last six-word line had been read.

   So both passes that tightened this cut the *mechanics* and left the
   *reading* nearly alone. Typing, the gaps between beats, and the terminal
   hold are the parts a visitor waits through; the open gate and the snapshot
   line are the two moments the section exists to be read. The run is 6.6s
   against the 11.6s it started at, and the two reading beats gave up 15% of
   that while the mechanics gave up half.

   That ratio is the rule, not a coincidence. A demo of a guardrail that
   nobody can read has lost its argument, so the next second has to come out
   of the waiting, not out of the two lines that carry the claim — and there
   is not much waiting left.

   Held here as one table rather than inline so the shape of the run is
   legible at a glance, and so both the rail and the clearing fade can be
   derived from it instead of guessed — see below. */
const TYPE_MS = 20

const BEATS = {
  // Dead air before the agent starts, so the terminal reads as idle first.
  leadIn: 260,
  // The command has landed; the gate has not fired yet. Cause before effect.
  commandSettles: 200,
  gateFires: 140,
  /* The gate open, with two lines to read. The panel's own entrance spends
     0.62s of this, so a little over a second is the actual read — this is the
     floor, and the beat that may not be cut to hit a target length. */
  gateHeld: 1700,
  // A human pressed Y. Long enough to register as a decision, not a flicker.
  decision: 360,
  // The snapshot line plus its sub-line — the second thing worth reading.
  snapshotRead: 1350,
  undoSettles: 300,
  /* The closing hold. It was 2900 once — a quarter of the whole cycle spent on
     one six-word line that is already read by the time it lands. It only has
     to be long enough to feel like an ending rather than a cut. */
  restored: 1150,
  clear: 340,
}

/* The rail under the title bar reports how far through the replay the visitor
   is, so its duration has to *be* the replay's duration. Hardcoded, it had
   drifted to 13.8s against an 11.6s cycle: the rail reset at 84% every single
   pass and never once reached its own end. Summed from the table, it cannot
   drift again — retune any beat above and the rail follows. */
const LOOP_MS =
  Object.values(BEATS).reduce((total, ms) => total + ms, 0) +
  (RISKY_CMD.length + UNDO_CMD.length) * TYPE_MS

/* The margin label is the gate made visible. Held is the only chromatic text
   in the section, so it has to clear 4.5:1 at 11px against the terminal — the
   panel's border keeps the deeper oxide. */
const NOTE_COLOR = {
  held: 'var(--color-electric)',
  allowed: 'var(--color-cyan-neon)',
}

const FINAL_FRAME = {
  rows: [
    {
      id: 'risky',
      kind: 'cmd',
      who: 'agent',
      text: RISKY_CMD,
      shown: RISKY_CMD.length,
      note: 'allowed',
    },
  ],
  panel: 'allowed',
  snapshot: true,
  undo: { shown: UNDO_CMD.length },
  restored: true,
  held: false,
}

const EMPTY = {
  rows: [],
  panel: null,
  snapshot: false,
  undo: null,
  restored: false,
  held: false,
}

/* The result mark used to be the literal "✓" (U+2713). Every face this page
   self-hosts is a Latin subset, so that codepoint fell outside every declared
   `unicode-range` and was drawn by whatever the OS offered — Segoe UI on
   Windows — at a width the 7ch prompt column had not budgeted for. Drawn
   instead, it is the same mark on every machine and it inherits the row's
   colour and size like the text beside it. */
function CheckMark() {
  return (
    <svg
      width="0.75em"
      height="0.75em"
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth="3"
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
      className="inline-block align-[-0.02em]"
    >
      <path d="M20 6 9 17l-5-5" />
    </svg>
  )
}

/* Prompts and result marks share one fixed column, so every command and every
   line Aegis prints starts at the same x — the way a real prompt behaves. */
function Gutter({ who, mark }) {
  return (
    <span className="w-[7ch] shrink-0 select-none text-cloud-dim">
      {who ? `${who} $` : mark ? <span className="pl-[3ch] text-cloud">{mark}</span> : ''}
    </span>
  )
}

export function GateDemo() {
  const [stageRef, inView] = useInView(0.3)
  const [frame, setFrame] = useState(EMPTY)
  const [clearing, setClearing] = useState(false)
  const [cycle, setCycle] = useState(0)
  const runRef = useRef(0)

  useEffect(() => {
    if (window.matchMedia('(prefers-reduced-motion: reduce)').matches) {
      setFrame(FINAL_FRAME)
      return
    }
    if (!inView) return

    const run = ++runRef.current
    const alive = () => runRef.current === run
    const sleep = (ms) => new Promise((r) => setTimeout(r, ms))

    /* Typing is per-character so the command reads as authored rather than
       pasted in. The caret rides the row while it grows. */
    const typeCmd = async (id, who, text, note) => {
      setFrame((f) => ({
        ...f,
        rows: [...f.rows, { id, kind: 'cmd', who, text, shown: 0, note: null }],
      }))
      for (let c = 1; c <= text.length; c++) {
        await sleep(TYPE_MS)
        if (!alive()) return false
        setFrame((f) => ({
          ...f,
          rows: f.rows.map((r) => (r.id === id ? { ...r, shown: c } : r)),
        }))
      }
      await sleep(BEATS.commandSettles)
      if (!alive()) return false
      if (note) {
        setFrame((f) => ({
          ...f,
          rows: f.rows.map((r) => (r.id === id ? { ...r, note } : r)),
        }))
      }
      return true
    }

    ;(async () => {
      while (alive()) {
        setClearing(false)
        setFrame(EMPTY)
        setCycle((n) => n + 1)
        await sleep(BEATS.leadIn)
        if (!alive()) return

        /* Beat one: straight into the action. The gate label flips before the
           dialog opens, so cause reads ahead of effect. */
        if (!(await typeCmd('risky', 'agent', RISKY_CMD, 'held'))) return
        setFrame((f) => ({ ...f, held: true }))
        await sleep(BEATS.gateFires)
        if (!alive()) return
        setFrame((f) => ({ ...f, panel: 'open' }))
        await sleep(BEATS.gateHeld)
        if (!alive()) return

        /* Beat two: a human says yes — and only then does the safety net
           appear. This is the whole product in one transition. */
        setFrame((f) => ({
          ...f,
          panel: 'allowed',
          held: false,
          rows: f.rows.map((r) => (r.id === 'risky' ? { ...r, note: 'allowed' } : r)),
        }))
        await sleep(BEATS.decision)
        if (!alive()) return
        setFrame((f) => ({ ...f, snapshot: true }))
        await sleep(BEATS.snapshotRead)
        if (!alive()) return

        /* Beat three: the way back is one command, typed by the human. */
        setFrame((f) => ({ ...f, undo: { shown: 0 } }))
        for (let c = 1; c <= UNDO_CMD.length; c++) {
          await sleep(TYPE_MS)
          if (!alive()) return
          setFrame((f) => ({ ...f, undo: { shown: c } }))
        }
        await sleep(BEATS.undoSettles)
        if (!alive()) return
        setFrame((f) => ({ ...f, restored: true }))
        await sleep(BEATS.restored)
        if (!alive()) return

        setClearing(true)
        await sleep(BEATS.clear)
      }
    })()

    return () => {
      runRef.current++
    }
  }, [inView])

  const last = frame.rows[frame.rows.length - 1]
  const typingCmd = last?.kind === 'cmd' && last.shown < last.text.length
  const typingUndo = frame.undo && frame.undo.shown < UNDO_CMD.length
  const idle = frame.rows.length === 0

  return (
    <section
      id="gate"
      aria-label="Aegis snapshotting before a destructive command"
      className="py-24 md:py-36"
    >
      <p className="sr-only">{SUMMARY}</p>

      <div className="mx-auto w-full max-w-[1200px] px-6" aria-hidden="true">
        <div
          ref={stageRef}
          className="mx-auto w-full max-w-[820px] overflow-hidden rounded-xl border border-night-rim/80 bg-night-void shadow-[0_28px_80px_rgba(0,0,0,0.55)]"
        >
          {/* Chrome. The status word is the only thing here that moves, and it
              moves for one reason: a command is waiting on a human. */}
          <div className="relative flex items-center justify-between border-b border-night-rim/60 px-4 py-3 md:px-5">
            <div className="flex items-center gap-2.5">
              <svg width="15" height="15" viewBox="0 0 17 17" fill="none" aria-hidden="true">
                <path
                  d="M8.5 1.5 14.5 4.8v7.4L8.5 15.5 2.5 12.2V4.8L8.5 1.5Z"
                  className="stroke-cloud-mute"
                />
                <path
                  d="M8.5 5.1v6.8M5.7 6.6 8.5 5l2.8 1.6"
                  className="stroke-cloud"
                />
              </svg>
              <span className="font-berkeley-mono text-[11px] tracking-[-0.01em] text-cloud-dim md:text-xs">
                aegis — zsh
              </span>
            </div>

            <div className="flex items-center gap-2 font-berkeley-mono text-[11px] tracking-[0.07em] uppercase md:text-xs">
              <span
                className="h-1.5 w-1.5 rounded-full transition-colors duration-300"
                style={{
                  backgroundColor: frame.held
                    ? 'var(--color-electric)'
                    : 'var(--color-cyan-neon)',
                }}
              />
              <span
                className="transition-colors duration-300"
                style={{
                  color: frame.held ? 'var(--color-cloud)' : 'var(--color-cloud-dim)',
                }}
              >
                {frame.held ? 'waiting for you' : 'shell guarded'}
              </span>
            </div>

            {/* A hairline scrubber keyed to the cycle: this replays. */}
            <div className="absolute -bottom-px left-0 h-px w-full">
              <div
                key={cycle}
                className="gate-loop-rail h-full w-full bg-cyan-neon/30"
                style={{ animationDuration: `${LOOP_MS}ms` }}
              />
            </div>
          </div>

          {/* Fixed height so nothing below the terminal moves as lines land. */}
          <div
            className={`gate-body min-h-[364px] px-4 py-5 sm:px-6 md:min-h-[392px] md:px-7 md:py-7${
              clearing ? ' is-clearing' : ''
            }`}
            /* The fade is given exactly the beat the run waits out for it, so
               the loop can never reset on a half-cleared body. */
            style={{ '--gate-clear': `${BEATS.clear}ms` }}
          >
            <div className="space-y-2.5 font-berkeley-mono text-[13px] leading-[1.6] sm:text-[15px] md:space-y-3 md:text-base">
              {idle && (
                <div className="flex items-baseline">
                  <Gutter who="agent" />
                  <span className="gate-caret" />
                </div>
              )}

              {frame.rows.map((row) => (
                <div key={row.id} className="gate-in flex items-baseline gap-3">
                  <Gutter who={row.who} />
                  <span className="min-w-0 flex-1 break-all text-cloud">
                    {row.text.slice(0, row.shown)}
                    {row === last && typingCmd && <span className="gate-caret" />}
                  </span>
                  {row.note && (
                    <span
                      key={row.note}
                      className="gate-in shrink-0 pt-px text-[11px] tracking-[0.08em] uppercase md:text-xs"
                      style={{ color: NOTE_COLOR[row.note] }}
                    >
                      {row.note}
                    </span>
                  )}
                </div>
              ))}

              {frame.panel && (
                <div
                  className="gate-panel-in mt-4 rounded-md border border-electric/50 bg-night/55 px-4 py-4 md:mt-5 md:px-5"
                >
                  <p className="text-[13px] text-cloud sm:text-[15px] md:text-base">
                    Aegis stopped this command.
                  </p>
                  <p className="mt-1.5 text-[12px] text-cloud-dim sm:text-[13px] md:text-sm">
                    It deletes your whole src folder.
                  </p>
                  <div className="mt-4 flex items-center gap-2.5">
                    <span
                      className={`rounded-sm border px-2.5 py-1.5 text-[11px] transition-colors duration-150 md:text-xs${
                        frame.panel === 'allowed' ? ' key-pressed' : ''
                      }`}
                      style={
                        frame.panel === 'allowed'
                          ? {
                              borderColor: 'var(--color-electric)',
                              backgroundColor: 'var(--color-electric)',
                              color: 'var(--color-night-void)',
                            }
                          : {
                              borderColor: 'var(--color-night-edge)',
                              color: 'var(--color-cloud)',
                            }
                      }
                    >
                      Y&nbsp; allow
                    </span>
                    <span className="rounded-sm border border-night-rim px-2.5 py-1.5 text-[11px] text-cloud-dim md:text-xs">
                      N&nbsp; deny
                    </span>
                  </div>
                </div>
              )}

              {/* The one moment the page is selling. It gets the brightest text
                  in the terminal and the only mark that is not a prompt. */}
              {frame.snapshot && (
                <div className="gate-in flex items-baseline gap-3 pt-2.5 md:pt-3">
                  <Gutter mark={<CheckMark />} />
                  <span className="min-w-0 flex-1">
                    <span className="text-cloud">{SNAPSHOT_LINE}</span>
                    <span className="mt-0.5 block text-[12px] text-cloud-dim sm:text-[13px] md:text-sm">
                      undo it any time
                    </span>
                  </span>
                </div>
              )}

              {frame.undo && (
                <div className="gate-in flex items-baseline gap-3 pt-1.5">
                  <Gutter who="you" />
                  <span className="min-w-0 flex-1 break-all text-cloud">
                    {UNDO_CMD.slice(0, frame.undo.shown)}
                    {typingUndo && <span className="gate-caret" />}
                  </span>
                </div>
              )}

              {frame.restored && (
                <div className="gate-in flex items-baseline gap-3">
                  <Gutter mark={<CheckMark />} />
                  <span className="min-w-0 flex-1 text-cloud">{RESTORED_LINE}</span>
                </div>
              )}
            </div>
          </div>
        </div>
      </div>
    </section>
  )
}
