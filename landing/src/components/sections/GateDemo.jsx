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

const TYPE_MS = 42

/* The loop only ever adds — a line that has been read is never taken away
   mid-run, the way a real scrollback behaves. The single removal is the reset
   at the end of the cycle. The rail under the title bar is keyed to this. */
const LOOP_MS = 13800

const SNAPSHOT_LINE = 'snapshot saved before it ran'
const RESTORED_LINE = 'src/ restored — nothing was lost'

/* The margin label is the gate made visible. Held is the only chromatic text
   in the section, so it has to clear 4.5:1 at 10px against the terminal — the
   panel's border keeps the deeper oxide. */
const NOTE_COLOR = { held: '#c9737b', allowed: '#718b9b' }

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

/* Prompts and result marks share one fixed column, so every command and every
   line Aegis prints starts at the same x — the way a real prompt behaves. */
function Gutter({ who, mark }) {
  return (
    <span className="w-[7ch] shrink-0 select-none text-steel">
      {who ? `${who} $` : mark ? <span className="pl-[3ch] text-haze">{mark}</span> : ''}
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
      await sleep(320)
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
        await sleep(650)
        if (!alive()) return

        /* Beat one: straight into the action. The gate label flips before the
           dialog opens, so cause reads ahead of effect. */
        if (!(await typeCmd('risky', 'agent', RISKY_CMD, 'held'))) return
        setFrame((f) => ({ ...f, held: true }))
        await sleep(240)
        if (!alive()) return
        setFrame((f) => ({ ...f, panel: 'open' }))
        await sleep(2100)
        if (!alive()) return

        /* Beat two: a human says yes — and only then does the safety net
           appear. This is the whole product in one transition. */
        setFrame((f) => ({
          ...f,
          panel: 'allowed',
          held: false,
          rows: f.rows.map((r) => (r.id === 'risky' ? { ...r, note: 'allowed' } : r)),
        }))
        await sleep(680)
        if (!alive()) return
        setFrame((f) => ({ ...f, snapshot: true }))
        await sleep(1900)
        if (!alive()) return

        /* Beat three: the way back is one command, typed by the human. */
        setFrame((f) => ({ ...f, undo: { shown: 0 } }))
        for (let c = 1; c <= UNDO_CMD.length; c++) {
          await sleep(TYPE_MS)
          if (!alive()) return
          setFrame((f) => ({ ...f, undo: { shown: c } }))
        }
        await sleep(560)
        if (!alive()) return
        setFrame((f) => ({ ...f, restored: true }))
        await sleep(2900)
        if (!alive()) return

        setClearing(true)
        await sleep(520)
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
          className="mx-auto w-full max-w-[820px] overflow-hidden rounded-xl border border-gunmetal/80 bg-pitch shadow-[0_28px_80px_rgba(0,0,0,0.55)]"
        >
          {/* Chrome. The status word is the only thing here that moves, and it
              moves for one reason: a command is waiting on a human. */}
          <div className="relative flex items-center justify-between border-b border-gunmetal/60 px-4 py-3 md:px-5">
            <div className="flex items-center gap-2.5">
              <svg width="15" height="15" viewBox="0 0 17 17" fill="none" aria-hidden="true">
                <path d="M8.5 1.5 14.5 4.8v7.4L8.5 15.5 2.5 12.2V4.8L8.5 1.5Z" stroke="#718b9b" />
                <path d="M8.5 5.1v6.8M5.7 6.6 8.5 5l2.8 1.6" stroke="#b3c2cb" />
              </svg>
              <span className="font-berkeley-mono text-[11px] tracking-[-0.01em] text-steel md:text-xs">
                aegis — zsh
              </span>
            </div>

            <div className="flex items-center gap-2 font-berkeley-mono text-[10px] tracking-[0.07em] uppercase md:text-[11px]">
              <span
                className="h-1.5 w-1.5 rounded-full transition-colors duration-300"
                style={{ backgroundColor: frame.held ? '#8d3a42' : '#718b9b' }}
              />
              <span
                className="transition-colors duration-300"
                style={{ color: frame.held ? '#b3c2cb' : '#718b9b' }}
              >
                {frame.held ? 'waiting for you' : 'shell guarded'}
              </span>
            </div>

            {/* A hairline scrubber keyed to the cycle: this replays. */}
            <div className="absolute -bottom-px left-0 h-px w-full">
              <div
                key={cycle}
                className="gate-loop-rail h-full w-full bg-tidal/30"
                style={{ animationDuration: `${LOOP_MS}ms` }}
              />
            </div>
          </div>

          {/* Fixed height so nothing below the terminal moves as lines land. */}
          <div
            className={`gate-body min-h-[364px] px-4 py-5 sm:px-6 md:min-h-[392px] md:px-7 md:py-7${
              clearing ? ' is-clearing' : ''
            }`}
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
                  <span className="min-w-0 flex-1 break-all text-haze">
                    {row.text.slice(0, row.shown)}
                    {row === last && typingCmd && <span className="gate-caret" />}
                  </span>
                  {row.note && (
                    <span
                      key={row.note}
                      className="gate-in shrink-0 pt-px text-[10px] tracking-[0.08em] uppercase md:text-[11px]"
                      style={{ color: NOTE_COLOR[row.note] }}
                    >
                      {row.note}
                    </span>
                  )}
                </div>
              ))}

              {frame.panel && (
                <div
                  className="gate-panel-in mt-4 rounded-md border px-4 py-4 md:mt-5 md:px-5"
                  style={{ borderColor: '#50252c', backgroundColor: 'rgba(41,22,27,0.55)' }}
                >
                  <p className="text-[13px] text-haze sm:text-[15px] md:text-base">
                    Aegis stopped this command.
                  </p>
                  <p className="mt-1.5 text-[12px] text-steel sm:text-[13px] md:text-sm">
                    It deletes your whole src folder.
                  </p>
                  <div className="mt-4 flex items-center gap-2.5">
                    <span
                      className={`rounded-sm border px-2.5 py-1.5 text-[11px] transition-colors duration-150 md:text-xs${
                        frame.panel === 'allowed' ? ' key-pressed' : ''
                      }`}
                      style={
                        frame.panel === 'allowed'
                          ? { borderColor: '#b3c2cb', backgroundColor: '#b3c2cb', color: '#06080c' }
                          : { borderColor: '#3e525f', color: '#b3c2cb' }
                      }
                    >
                      Y&nbsp; allow
                    </span>
                    <span className="rounded-sm border border-gunmetal px-2.5 py-1.5 text-[11px] text-steel md:text-xs">
                      N&nbsp; deny
                    </span>
                  </div>
                </div>
              )}

              {/* The one moment the page is selling. It gets the brightest text
                  in the terminal and the only mark that is not a prompt. */}
              {frame.snapshot && (
                <div className="gate-in flex items-baseline gap-3 pt-2.5 md:pt-3">
                  <Gutter mark="✓" />
                  <span className="min-w-0 flex-1">
                    <span className="text-haze">{SNAPSHOT_LINE}</span>
                    <span className="mt-0.5 block text-[12px] text-steel sm:text-[13px] md:text-sm">
                      undo it any time
                    </span>
                  </span>
                </div>
              )}

              {frame.undo && (
                <div className="gate-in flex items-baseline gap-3 pt-1.5">
                  <Gutter who="you" />
                  <span className="min-w-0 flex-1 break-all text-haze">
                    {UNDO_CMD.slice(0, frame.undo.shown)}
                    {typingUndo && <span className="gate-caret" />}
                  </span>
                </div>
              )}

              {frame.restored && (
                <div className="gate-in flex items-baseline gap-3">
                  <Gutter mark="✓" />
                  <span className="min-w-0 flex-1 text-haze">{RESTORED_LINE}</span>
                </div>
              )}
            </div>
          </div>
        </div>
      </div>
    </section>
  )
}
